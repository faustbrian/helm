use crate::config::{Driver, ServiceConfig};

pub(super) fn append(args: &mut Vec<String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Postgres => append_postgres(args, service),
        Driver::Mysql => append_mysql(args, service),
        _ => {}
    }
}

fn append_postgres(args: &mut Vec<String>, service: &ServiceConfig) {
    args.push("-e".to_owned());
    args.push(format!(
        "POSTGRES_USER={}",
        service.username.as_deref().unwrap_or("postgres")
    ));
    args.push("-e".to_owned());
    args.push(format!(
        "POSTGRES_PASSWORD={}",
        service.password.as_deref().unwrap_or("secret")
    ));
    args.push("-e".to_owned());
    args.push(format!(
        "POSTGRES_DB={}",
        service.database.as_deref().unwrap_or("app")
    ));
}

fn append_mysql(args: &mut Vec<String>, service: &ServiceConfig) {
    let password = service.password.as_deref().unwrap_or("secret");
    args.push("-e".to_owned());
    args.push(format!("MYSQL_ROOT_PASSWORD={password}"));
    args.push("-e".to_owned());
    args.push(format!(
        "MYSQL_DATABASE={}",
        service.database.as_deref().unwrap_or("app")
    ));
    args.push("-e".to_owned());
    args.push(format!(
        "MYSQL_USER={}",
        service.username.as_deref().unwrap_or("root")
    ));
    args.push("-e".to_owned());
    args.push(format!("MYSQL_PASSWORD={password}"));
}
