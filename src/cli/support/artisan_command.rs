//! Shared builders for `php artisan` command invocation.

pub(crate) fn build_artisan_command(user_command: Vec<String>) -> Vec<String> {
    let mut full_command = vec!["php".to_owned()];
    full_command.push("artisan".to_owned());
    full_command.extend(user_command);
    full_command
}

pub(crate) fn build_artisan_subcommand(subcommand: &str, args: &[&str]) -> Vec<String> {
    let mut command = Vec::with_capacity(3 + args.len());
    command.push(subcommand.to_owned());
    command.extend(args.iter().map(|value| (*value).to_owned()));
    build_artisan_command(command)
}
