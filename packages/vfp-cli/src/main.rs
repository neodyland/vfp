mod client;
mod options;
mod server;

#[tokio::main]
pub async fn main() {
    let options = options::parse();
    match options.subcommands {
        options::Subcommands::Serve(options) => {
            server::main(options.port).await;
        }
        options::Subcommands::Client(options) => {
            client::main(options.addr, options.message).await;
        }
    }
}
