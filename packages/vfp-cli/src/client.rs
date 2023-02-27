pub async fn main(addr: String, message: String) {
    let client = vfp::connect(&addr).await;
    if client.is_err() {
        return;
    }
    let mut client = client.unwrap();
    let io = client.open().await;
    if io.is_err() {
        return;
    }
    let mut io = io.unwrap();
    match io.write(message.as_bytes().to_vec()).await {
        Ok(_) => {}
        Err(_) => {
            return;
        }
    };
    let chunk = io.read().await;
    if chunk.is_err() {
        return;
    }
    let chunk = chunk.unwrap();
    println!("Received chunk: {:?}", chunk);
    match io.close().await {
        Ok(_) => {}
        Err(_) => {
            return;
        }
    };
}
