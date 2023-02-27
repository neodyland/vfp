use clap::Parser;

#[derive(Debug, Parser)]
pub struct Options {
    #[clap(subcommand)]
    pub subcommands: Subcommands,
}

#[derive(Debug, Parser)]
pub enum Subcommands {
    Serve(Serve),
    Client(Client),
}

#[derive(Debug, Parser)]
pub struct Serve {
    #[clap(short, long)]
    pub port: u16,
}

#[derive(Debug, Parser)]
pub struct Client {
    #[clap(short, long)]
    pub addr: String,
    #[clap(short, long)]
    pub message: String,
}

pub fn parse() -> Options {
    Options::parse()
}
