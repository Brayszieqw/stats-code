fn main() {
    if let Err(error) = stats_code::run() {
        eprintln!(
            "error: {error}

Run `stats-code --help` for usage."
        );
        std::process::exit(1);
    }
}
