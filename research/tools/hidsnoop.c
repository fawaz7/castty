/*
 * hidsnoop -- LD_PRELOAD shim that logs hidraw feature reports.
 *
 * Wine translates the Windows HidD_SetFeature/HidD_GetFeature calls the Mionix
 * vendor application makes into HIDIOCSFEATURE/HIDIOCGFEATURE ioctls on
 * /dev/hidraw*. Wrapping ioctl() therefore captures exactly the buffers the
 * application passed, already framed one report per entry -- which usbmon does
 * not give you, and which needs no root.
 *
 *   cc -shared -fPIC -O2 -o hidsnoop.so hidsnoop.c -ldl
 *   LD_PRELOAD=./hidsnoop.so HIDSNOOP_LOG=cap.log wine "CASTOR Software.exe"
 *
 * Every capture in ../captures/sessions/ was produced by this file. The log is
 * flushed per report because the vendor app is unstable under Wine and a capture
 * has to survive it hanging. Decode the result with decode_capture.py.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <time.h>
#include <unistd.h>

/* From linux/hidraw.h, spelled out so this builds without kernel headers. */
#define HIDRAW_IOC_TYPE 'H'
#define HIDRAW_NR_SFEATURE 0x06
#define HIDRAW_NR_GFEATURE 0x07

/* A profile blob is 1041 bytes; this is ample and keeps the shim allocation-free
 * on the hot path, which matters because ioctl() interposition sees everything
 * the process does, not just hidraw. */
#define SNAP_MAX 4096

static FILE *log_fp;

static FILE *logfile(void)
{
    if (!log_fp) {
        const char *path = getenv("HIDSNOOP_LOG");
        log_fp = path ? fopen(path, "a") : NULL;
        if (!log_fp)
            log_fp = stderr;
    }
    return log_fp;
}

static double now(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + ts.tv_nsec / 1e9;
}

/*
 * Canonical 16-bytes-per-line hexdump with an ASCII gutter. decode_capture.py
 * slices columns 8..55 out of these lines, so the spacing is load-bearing --
 * do not reformat it without updating the decoder to match.
 */
static void dump(FILE *fp, const unsigned char *buf, size_t len)
{
    for (size_t off = 0; off < len; off += 16) {
        fprintf(fp, "  %04zx  ", off);
        for (size_t i = 0; i < 16; i++) {
            if (off + i < len)
                fprintf(fp, "%02x", buf[off + i]);
            else
                fputs("  ", fp);
            if (i != 15)
                fputc(' ', fp);
        }
        fputs("  |", fp);
        for (size_t i = 0; i < 16 && off + i < len; i++) {
            unsigned char c = buf[off + i];
            fputc(c >= 0x20 && c < 0x7f ? c : '.', fp);
        }
        fputs("|\n", fp);
    }
}

int ioctl(int fd, unsigned long request, ...)
{
    static int (*real)(int, unsigned long, ...);
    if (!real)
        real = dlsym(RTLD_NEXT, "ioctl");

    va_list ap;
    va_start(ap, request);
    void *arg = va_arg(ap, void *);
    va_end(ap);

    /* Only the two vendor feature-report ioctls are interesting; everything else
     * -- and a Wine process issues a great deal of it -- passes straight through. */
    unsigned nr = _IOC_NR(request);
    int is_set = _IOC_TYPE(request) == HIDRAW_IOC_TYPE && nr == HIDRAW_NR_SFEATURE;
    int is_get = _IOC_TYPE(request) == HIDRAW_IOC_TYPE && nr == HIDRAW_NR_GFEATURE;
    if ((!is_set && !is_get) || !arg)
        return real(fd, request, arg);

    size_t len = _IOC_SIZE(request);

    /* A write is only meaningful as it was *before* the call -- the driver is
     * free to clobber the buffer -- so snapshot it first and report it after,
     * once the return value is known. A read is meaningful only afterwards. */
    unsigned char snap[SNAP_MAX];
    size_t snap_len = len < SNAP_MAX ? len : SNAP_MAX;
    if (is_set)
        memcpy(snap, arg, snap_len);

    int ret = real(fd, request, arg);

    FILE *fp = logfile();
    fprintf(fp, "[%.6f] %s fd=%d len=%zu ret=%d\n",
            now(), is_set ? "SET_FEATURE" : "GET_FEATURE", fd, len, ret);
    dump(fp, is_set ? snap : (const unsigned char *)arg, snap_len);
    fflush(fp);
    return ret;
}

/*
 * Logging opens as well makes the fd numbers above identifiable: without it you
 * cannot tell which hidraw node -- and so which interface -- a given fd is.
 * Wine reaches both open() and open64() depending on the build, so wrap both.
 */
static int wrap_open(int fd, const char *path)
{
    if (path && strstr(path, "hidraw")) {
        FILE *fp = logfile();
        fprintf(fp, "[open] %s -> fd=%d\n", path, fd);
        fflush(fp);
    }
    return fd;
}

int open(const char *path, int flags, ...)
{
    static int (*real)(const char *, int, ...);
    if (!real)
        real = dlsym(RTLD_NEXT, "open");

    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = va_arg(ap, int);
        va_end(ap);
    }
    return wrap_open(real(path, flags, mode), path);
}

int open64(const char *path, int flags, ...)
{
    static int (*real)(const char *, int, ...);
    if (!real)
        real = dlsym(RTLD_NEXT, "open64");

    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list ap;
        va_start(ap, flags);
        mode = va_arg(ap, int);
        va_end(ap);
    }
    return wrap_open(real(path, flags, mode), path);
}
