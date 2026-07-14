use apk_installer::cli::Args;
use apk_installer::run;
use clap::Parser;
use std::process;

fn main() {
    let args = Args::parse();

    if let Err(e) = run(args) {
        eprintln!("错误：{}", e);
        process::exit(1);
    }
}
