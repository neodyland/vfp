pub async fn main(port: u16) {
    let server = vfp::listen(&port).await;
    if server.is_err() {
        return;
    }
    let mut server = server.unwrap();
    println!("Listening on port {}", port);
    loop {
        match server.accept().await {
            Ok((mut session, addr)) => {
                tokio::spawn(async move {
                    println!("Accepted connection from {}", addr);
                    loop {
                        match session.next().await {
                            Some(mut io) => {
                                println!("Accepted io from {}", addr);
                                tokio::spawn(async move {
                                    match io.read().await {
                                        Ok(chunk) => {
                                            println!("Received chunk: {:?}", chunk);
                                            match io.write(chunk).await {
                                                Ok(_) => {}
                                                Err(_) => {
                                                    return;
                                                }
                                            };
                                        }
                                        Err(_) => {
                                            return;
                                        }
                                    }
                                });
                            }
                            None => break,
                        }
                    }
                });
            }
            Err(_) => {
                break;
            }
        }
    }
}
