pub enum CliCommand {
    Help,
    Register { user: String, password: String, server: String },
    Login { user: String, password: String, server: String },
    Logout { server: Option<String> },
    Status,
    Chat { server: Option<String> },
}

pub fn parse_args<I>(args: I) -> Result<CliCommand, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(CliCommand::Help);
    };

    let rest = args.collect::<Vec<_>>();
    match command.as_str() {
        "help" | "--help" | "-h" => Ok(CliCommand::Help),
        "register" => Ok(CliCommand::Register {
            user: required_flag(&rest, "--user")?,
            password: required_flag(&rest, "--password")?,
            server: find_flag(&rest, "--server").unwrap_or_else(default_server),
        }),
        "login" => Ok(CliCommand::Login {
            user: required_flag(&rest, "--user")?,
            password: required_flag(&rest, "--password")?,
            server: find_flag(&rest, "--server").unwrap_or_else(default_server),
        }),
        "logout" => Ok(CliCommand::Logout {
            server: find_flag(&rest, "--server"),
        }),
        "status" => Ok(CliCommand::Status),
        "chat" => Ok(CliCommand::Chat {
            server: find_flag(&rest, "--server"),
        }),
        other => Err(format!("unknown command: {other}")),
    }
}

pub fn print_help() {
    println!("messanger");
    println!();
    println!("commands:");
    println!("  register --user <name> --password <pass> [--server <url>]");
    println!("  login    --user <name> --password <pass> [--server <url>]");
    println!("  logout   [--server <url>]");
    println!("  status");
    println!("  chat     [--server <url>]");
    println!();
    println!("inside chat:");
    println!("  --help                         show chat commands");
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
    println!("  --logout                       deauthorize this PC and exit chat");
    println!("  --quit                         exit chat");
    println!("  any other text                 encrypt locally and send ciphertext to the server");
    println!();
    println!("notes:");
    println!("  - active key starts as start");
    println!("  - message length is limited to 80 characters");
    println!("  - default server is {}", default_server());
    println!("  - the client encrypts locally before upload");
    println!("  - the server stores and returns only ciphertext");
}

pub fn print_chat_help() {
    println!("chat commands:");
    println!("  --help");
    println!("  --key <secret>");
    println!("  --create-room <name> [limit]");
    println!("  --join-room <name>");
    println!("  --leave-room");
    println!("  --rooms");
    println!("  --members");
    println!("  --set-limit <number>");
    println!("  --kick <user>");
    println!("  --ban <user>");
    println!("  --refresh");
    println!("  --logout");
    println!("  --quit");
    println!("  message text up to 80 characters");
}

fn required_flag(args: &[String], flag: &str) -> Result<String, String> {
    find_flag(args, flag).ok_or_else(|| format!("missing required flag: {flag}"))
}

fn find_flag(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn default_server() -> String {
    "http://127.0.0.1:25655".to_string()
}
