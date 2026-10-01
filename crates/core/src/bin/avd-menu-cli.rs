//! Linha de comando sem interface gráfica (não depende de GTK/WebKit).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match avdcore::cli::run(&args) {
        Some(code) => std::process::exit(code),
        None => {
            eprintln!(
                "{}",
                avdcore::lang::tr(
                    "Esta versão não tem interface gráfica. Use `avd-menu --help`.",
                    "This build has no graphical interface. Use `avd-menu --help`."
                )
            );
            std::process::exit(2);
        }
    }
}
