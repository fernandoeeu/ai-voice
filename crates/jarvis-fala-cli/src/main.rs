//! `jarvis-fala-rs`: cliente fino do jarvisd. Esqueleto da F0.

use clap::Parser;

/// Fala um texto pelo jarvisd.
#[derive(Parser, Debug)]
#[command(name = "jarvis-fala-rs", version)]
struct Args {
    /// Texto a falar.
    texto: Vec<String>,
}

fn main() {
    let args = Args::parse();
    println!("jarvis-fala-rs: esqueleto da F0 ({} palavras)", args.texto.len());
}
