pub enum CliCommand {
    Help,
    Chat { user: String, server: String },
}

pub fn parse_args<I>(args: I) -> Result<CliCommand, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(CliCommand::Help);
    };

    match command.as_str() {
        "help" | "--help" | "-h" => Ok(CliCommand::Help),
        "chat" => parse_chat(args.collect()),
        other => Err(format!("unknown command: {other}")),
    }
}

pub fn print_help() {
    println!("messanger");
    println!();
    println!("commands:");
    println!("  chat --user <name> [--server <url>]");
    println!();
    println!("inside chat:");
    println!("  --key <secret>                 switch encryption key");
    println!("  --create-room <name> [limit]   create a room and join it");
    println!("  --join-room <name>             join a room and make it active");
    println!("  --leave-room                   leave the current room");
    println!("  --rooms                        list rooms");
    println!("  --members                      list members of the current room");
    println!("  --set-limit <number>           owner changes room member limit");
    println!("  --kick <user>                  owner kicks a member from the room");
    println!("  --ban <user>                   owner bans a member from the room");
    println!("  --refresh                      reload current room messages");
    println!("  --quit                         exit chat");
    println!("  any other text                 encrypt locally and send ciphertext to the server");
    println!();
    println!("notes:");
    println!("  - active key starts as start");
    println!("  - default server is http://127.0.0.1:25655");
    println!("  - rooms are independent from keys");
    println!("  - the client encrypts locally before upload");
    println!("  - the server stores and returns only ciphertext");
}

fn parse_chat(args: Vec<String>) -> Result<CliCommand, String> {
    let user = required_flag(&args, "--user")?;
    let server = find_flag(&args, "--server").unwrap_or_else(|| "http://127.0.0.1:25655".to_string());
    Ok(CliCommand::Chat { user, server })
}

fn required_flag(args: &[String], flag: &str) -> Result<String, String> {
    find_flag(args, flag).ok_or_else(|| format!("missing required flag: {flag}"))
}

fn find_flag(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}
