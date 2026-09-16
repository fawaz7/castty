//! Entry point. With no arguments it launches the iced GUI; with arguments it
//! is a command-line tool. Both drive the same `hardware` layer.

use castty::config;
use castty::hardware::Device;
use std::env;
use std::process::ExitCode;

/// Which profile the CLI acts on. The mouse has five; `-p N` selects one.
fn profile_index(args: &[String]) -> usize {
    args.iter()
        .position(|a| a == "-p")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1).min(config::PROFILE_COUNT - 1))
        .unwrap_or(0)
}

fn load_profile(index: usize) -> Result<castty::hardware::Profile, Box<dyn std::error::Error>> {
    Ok(config::load(index))
}

fn save_profile(index: usize, p: &castty::hardware::Profile) -> Result<(), Box<dyn std::error::Error>> {
    config::save(index, p)?;
    Ok(())
}

fn parse_hex_colour(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ))
}

fn usage() -> ExitCode {
    eprintln!(
        "castty -- Mionix Castor configuration

USAGE:
    castty info                 show device identity
    castty -p <1-5> <command>   act on a profile other than the first
    castty led <RRGGBB>         set both LEDs to a colour
    castty mode <effect> [rainbow]
                                solid, blinking, pulsating, breathing
    castty dpi <1|2|3> <value>  set a DPI step
    castty surface [seconds]    run the surface analyzer (default 10s of movement)
    castty reset                restore factory defaults

Settings are stored in {} because the device has no read-back path.",
        config::state_path(0).display()
    );
    ExitCode::from(2)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let index = profile_index(&args);
    let dev = Device::open()?;
    let _ = &dev;

    match args.first().map(String::as_str) {
        Some("info") => {
            let id = dev.identify()?;
            println!("device:   {}", dev.path().display());
            println!("firmware: 0x{:04x}", id.firmware);
            println!("mcu:      {}", id.mcu);
            let p = load_profile(index)?;
            println!(
                "profile:  {} | dpi {}/{}/{} | polling {} | snap {} | angle {}",
                p.name,
                p.dpi[0].x,
                p.dpi[1].x,
                p.dpi[2].x,
                p.polling.map(|r| r.hz()).unwrap_or(0),
                p.angle_snapping,
                p.angle_tuning
            );
        }
        Some("led") => {
            let (r, g, b) = args
                .get(1)
                .and_then(|s| parse_hex_colour(s))
                .ok_or("expected a colour like ff00ff")?;
            let mut p = load_profile(index)?;
            p.set_all_colours(r, g, b);
            dev.write_profile(&p)?;
            save_profile(index, &p)?;
            println!("LEDs set to {r:02x}{g:02x}{b:02x}");
        }
        Some("mode") => {
            let name = args.get(1).map(String::as_str).unwrap_or("").to_lowercase();
            let effect = castty::hardware::EFFECTS
                .iter()
                .find(|e| e.label().to_lowercase() == name)
                .copied()
                .ok_or_else(|| {
                    let names: Vec<String> = castty::hardware::EFFECTS
                        .iter()
                        .map(|e| e.label().to_lowercase())
                        .collect();
                    format!("expected one of: {}", names.join(", "))
                })?;
            // rainbow is a flag on the mode byte, so it layers onto any effect
            let rainbow = args.iter().any(|a| a == "rainbow");
            let mode = castty::hardware::LedMode::new(effect, rainbow);
            let mut p = load_profile(index)?;
            p.set_mode(mode);
            dev.write_profile(&p)?;
            save_profile(index, &p)?;
            println!("LED mode set to {}", mode.label());
        }
        Some("dpi") => {
            let step: usize = args.get(1).ok_or("expected step 1-3")?.parse()?;
            let value: u16 = args.get(2).ok_or("expected a DPI value")?.parse()?;
            if !(1..=3).contains(&step) {
                return Err("step must be 1, 2 or 3".into());
            }
            let mut p = load_profile(index)?;
            p.dpi[step - 1] = castty::hardware::DpiStep::linked(value);
            dev.write_profile(&p)?;
            save_profile(index, &p)?;
            println!("DPI step {step} set to {value}");
        }
        Some("surface") => {
            // The measurement needs the mouse moved across the surface while it
            // runs; reading straight away measures nothing.
            let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
            dev.surface_start()?;
            for remaining in (1..=secs).rev() {
                print!("\rmove the mouse over the surface... {remaining:2}s ");
                use std::io::Write;
                std::io::stdout().flush().ok();
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            let raw = dev.surface_result()?;
            println!(
                "\rsurface quality: {} / 10 (raw {raw})     ",
                castty::hardware::surface_score(raw)
            );
        }
        Some("reset") => {
            let mut p = config::factory_default(index);
            p.index = index as u8;
            dev.write_profile(&p)?;
            save_profile(index, &p)?;
            println!("factory defaults restored");
        }
        _ => return Err("unknown command".into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    // No arguments: launch the GUI. The CLI stays available for scripting and
    // for working on the hardware layer without a display.
    if env::args().len() <= 1 {
        return match castty::iced_ui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            if e.to_string() == "unknown command" {
                return usage();
            }
            ExitCode::FAILURE
        }
    }
}
