pub(super) fn build_logs_args(
    container_name: &str,
    follow: bool,
    tail: Option<u64>,
    timestamps: bool,
) -> Vec<String> {
    let mut args = vec!["logs".to_owned()];
    if follow {
        args.push("-f".to_owned());
    }
    if timestamps {
        args.push("--timestamps".to_owned());
    }
    if let Some(n) = tail {
        args.push("--tail".to_owned());
        args.push(n.to_string());
    }
    args.push(container_name.to_owned());
    args
}
