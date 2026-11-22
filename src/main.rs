//! Thin binary entry point: parse arguments, dispatch through the library,
//! print one stable diagnostic and exit with the View-owned exit code.

use clap::Parser;

fn main() {
    let cli = marqdo_view::Cli::parse();
    match marqdo_view::run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(err.exit_code());
        }
    }
}
