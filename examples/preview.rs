use std::path::Path;
use std::error::Error;
#[allow(unused_imports)]
use log::{trace, debug, info, warn, error};
use clap::{Command, Arg, ArgGroup, error::ErrorKind, value_parser, builder::NonEmptyStringValueParser};
use enigo::*;



fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let mut cmd = Command::new("preview")
        .about("An example of the macmatic framework")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("window_name")
                .short('w')
                .long("window")
                .num_args(1)
                .value_name("NAME")
                .help("Name of the window to capture (prefix with ~ for regexp)")
                .value_parser(NonEmptyStringValueParser::new())
        )
        .arg(
            Arg::new("window_id")
                .short('i')
                .long("id")
                .num_args(1)
                .value_name("ID")
                .help("Id of the window to capture")
                .value_parser(value_parser!(i64))
        )
        .group(ArgGroup::new("window")
               .args(["window_name", "window_id"])
        )
        .subcommand(
            Command::new("list")
                .about("list windows name")
        )
        .subcommand(
            Command::new("screenshot")
                .about("screenshot a window")
                .arg(
                    Arg::new("filename")
                    .short('f')
                    .long("file")
                    .num_args(1)
                    .value_name("FILENAME")
                    .required(true)
                    .help("Filename of the screenshot")
                    )
        )
        .subcommand(
            Command::new("test_find")
                .about("Search the template image in a window")
                .arg(
                    Arg::new("template")
                    .short('t')
                    .long("template")
                    .num_args(1)
                    .value_name("FILENAME")
                    .required(true)
                    .help("Filename of the template image")
                    )
        )
        .subcommand(
            Command::new("test_preview")
                .about("Example of automation of the Preview app")
        );
    let matches = cmd.get_matches_mut();

    let mut bot = macmatic::Bot::new();
    let enigo = Enigo::new();
    debug!("Display size: {:?}", enigo.main_display_size());
    bot.set_controller(enigo);

    match matches.subcommand() {
        Some(("list", _)) => {
            // list all windows
            print!("\n{}\n", macmatic::WindowList::new().prettify());
        }
        Some(("screenshot", sub_matches)) => {
            set_window_from_arg(&mut cmd, &mut bot);
            let file = Path::new(sub_matches.get_one::<String>("filename").unwrap());
            bot.window.as_ref().unwrap().screenshot(&file).unwrap();
            info!(
                "Screenshoting {:#?}", bot
            );
        }
        Some(("test_find", sub_matches)) => {
            set_window_from_arg(&mut cmd, &mut bot);
            let file = Path::new(sub_matches.get_one::<String>("template").unwrap());
            example_find(&mut bot, &file)?;
        }
        Some(("test_preview", _)) => {
            set_window_from_arg(&mut cmd, &mut bot);
            example_preview(&mut bot)?;
        }
        _ => unreachable!(), // If all subcommands are defined above, anything else is unreachabe!()
    }

    fn set_window_from_arg(cmd: &mut Command, bot: &mut macmatic::Bot) {
        let matches = cmd.get_matches_mut();
        if matches.contains_id("window_name") {
            let name = matches.get_one::<String>("window_name").unwrap();
            if name.starts_with('~') {
                let regex: &str = &name[1..name.len() - 1];
                bot.set_window_from_regex(regex);
            } else {
                bot.set_window_from_name(name);
            }
        } else if matches.contains_id("window_id") {
            let id: i64 = *matches.get_one::<i64>("window_id").expect("Invalid window Id");
            bot.set_window_from_id(id);
        } else {
            cmd.error(
                ErrorKind::MissingRequiredArgument,
                "Window name or id required"
                )
                .exit();
        }
        trace!("Window found: {:#?}", bot.window);
        if bot.window.is_none() {
            cmd.error(
                ErrorKind::InvalidValue,
                "Window not found"
                )
                .exit();
        }
    }

    Ok(())
}

fn example_find(bot: &mut macmatic::Bot, file: &Path) -> Result<(), Box<dyn Error>> {
    let wait_time = 800; // in millis

    bot.sleep(wait_time);
    let rect = bot.find(&file).unwrap();
    info!("Template found at {:#?}", rect);

    Ok(())
}

fn example_preview(bot: &mut macmatic::Bot) -> Result<(), Box<dyn Error>> {
    let wait_time = 800; // in millis

    bot.activate_window()?;
    bot.sleep(wait_time);
    let rect = bot.find(Path::new("examples/img/W.png")).unwrap();
    bot.mouse_down_on(rect.x, rect.y)?;
    bot.sleep(wait_time);
    bot.mouse_up_on(rect.x + rect.width, rect.y + rect.height)?;

    bot.sleep(wait_time);
    bot.key_down(Key::Control)?;
    bot.key_down(Key::Meta)?;
    bot.key_click(Key::Layout('T'))?;
    bot.key_up(Key::Meta)?;
    bot.key_up(Key::Control)?;
    bot.sleep(wait_time);

    bot.write("macmatic")?;
    bot.sleep(wait_time);

    if bot.click_on_image(Path::new("examples/img/W.png"), 500).is_err() {
            warn!("Failed to find and click on W.img");
    }

    Ok(())
}
