fn main() {
    let code = match etymonoetic_interlingua::cli::run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    };

    std::process::exit(code);
}
