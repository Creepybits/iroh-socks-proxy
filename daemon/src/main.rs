use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const SOCKS_VERSION: u8 = 0x05;
const METHOD_NO_AUTH: u8 = 0x00;
const CMD_CONNECT: u8 = 0x01;

const ADDR_TYPE_IPV4: u8 = 0x01;
const ADDR_TYPE_DOMAIN: u8 = 0x03;

const REP_SUCCESS: u8 = 0x00;
const REP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
const REP_ADDR_TYPE_NOT_SUPPORTED: u8 = 0x08;
const REP_GENERAL_FAILURE: u8 = 0x01;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listen_addr = "127.0.0.1:9999";
    let listener = TcpListener::bind(listen_addr).await?;
    println!("Sovereign daemon loopback proxy bound to {}", listen_addr);

    loop {
        // Accept incoming browser connections on the loopback interface
        let (stream, client_addr) = match listener.accept().await {
            Ok(val) => val,
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                continue;
            }
        };

        // Spawn an asynchronous task for each connection
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream).await {
                eprintln!("Error handling client {}: {}", client_addr, e);
            }
        });
    }
}

async fn handle_client(mut client_stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    // 1. SOCKS5 Method Selection Handshake
    let mut header = [0u8; 2];
    client_stream.read_exact(&mut header).await?;

    let version = header[0];
    let nmethods = header[1];

    if version != SOCKS_VERSION {
        return Err("Unsupported SOCKS version".into());
    }

    // Read supported methods
    let mut methods = vec![0u8; nmethods as usize];
    client_stream.read_exact(&mut methods).await?;

    // We only support No-Authentication (0x00) for this local loopback
    if !methods.contains(&METHOD_NO_AUTH) {
        // Return 0xFF to tell client no acceptable methods were found
        client_stream.write_all(&[SOCKS_VERSION, 0xFF]).await?;
        return Err("No acceptable authentication methods".into());
    }

    // Respond to client indicating No-Authentication method is selected
    client_stream.write_all(&[SOCKS_VERSION, METHOD_NO_AUTH]).await?;

    // 2. Read Client Request
    let mut request_header = [0u8; 4];
    client_stream.read_exact(&mut request_header).await?;

    let req_ver = request_header[0];
    let cmd = request_header[1];
    let atyp = request_header[3];

    if req_ver != SOCKS_VERSION {
        return Err("Invalid request SOCKS version".into());
    }

    if cmd != CMD_CONNECT {
        send_reply(&mut client_stream, REP_COMMAND_NOT_SUPPORTED).await?;
        return Err("Command not supported".into());
    }

    // Parse the target destination address
    let target_address = match atyp {
        ADDR_TYPE_IPV4 => {
            let mut ipv4_addr = [0u8; 4];
            client_stream.read_exact(&mut ipv4_addr).await?;
            let mut port_bytes = [0u8; 2];
            client_stream.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            
            format!("{}.{}.{}.{}:{}", ipv4_addr[0], ipv4_addr[1], ipv4_addr[2], ipv4_addr[3], port)
        }
        ADDR_TYPE_DOMAIN => {
            let len = client_stream.read_u8().await? as usize;
            let mut domain_bytes = vec![0u8; len];
            client_stream.read_exact(&mut domain_bytes).await?;
            let domain = String::from_utf8(domain_bytes)?;
            
            let mut port_bytes = [0u8; 2];
            client_stream.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);

            format!("{}:{}", domain, port)
        }
        _ => {
            send_reply(&mut client_stream, REP_ADDR_TYPE_NOT_SUPPORTED).await?;
            return Err("Address type not supported".into());
        }
    };

    println!("Intercepted request to: {}", target_address);


    // 3. Check if this is a custom Sovereign domain (.anon)
    let is_sovereign_domain = target_address.contains(".anon");

    if is_sovereign_domain {
        println!("⚡ [Sovereign Domain Detected]: Initiating P2P retrieval for {}", target_address);
        
        // 1. Tell the browser the SOCKS5 tunnel is established successfully
        send_reply(&mut client_stream, REP_SUCCESS).await?;
        
        // 2. Read the browser's incoming HTTP request headers first (swallowing the GET request)
        let mut browser_request = [0u8; 1024];
        let n = client_stream.read(&mut browser_request).await?;
        if n == 0 {
            return Ok(()); // Connection closed prematurely by the browser
        }
        
        // 3. Write our mock HTTP response over the open tunnel
        let mock_html = "HTTP/1.1 200 OK\r\n\
                         Content-Type: text/html; charset=utf-8\r\n\
                         Connection: close\r\n\r\n\
                         <!DOCTYPE html><html><head><title>Sovereign Web</title><style>body { background-color: #1a1a1a; color: #a855f7; font-family: sans-serif; text-align: center; padding-top: 100px; }</style></head>\
                         <body><h1>⚡ Welcome to the Sovereign Web ⚡</h1><p style='color:#ffffff;'>You are viewing this site completely decentralized, bypassed from standard DNS.</p></body></html>";
        
        client_stream.write_all(mock_html.as_bytes()).await?;
        
        // 4. Gracefully flush and shut down the write-half of the connection so Chrome receives everything
        client_stream.shutdown().await?;
        return Ok(());
    }

    // 4. Default Fallback: Connect to standard Clearnet
    match TcpStream::connect(&target_address).await {
        Ok(mut target_stream) => {
            send_reply(&mut client_stream, REP_SUCCESS).await?;
            if let Err(e) = tokio::io::copy_bidirectional(&mut client_stream, &mut target_stream).await {
                eprintln!("Error during bidirectional streaming: {}", e);
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to target {}: {}", target_address, e);
            send_reply(&mut client_stream, REP_GENERAL_FAILURE).await?;
        }
    }

    Ok(())
}

// Helper function to send SOCKS5 response header back to the client
async fn send_reply(client_stream: &mut TcpStream, rep: u8) -> Result<(), std::io::Error> {
    // SOCKS5 reply format: [VER, REP, RSV (0x00), ATYP (IPv4), BND.ADDR (0.0.0.0), BND.PORT (0)]
    let reply = [
        SOCKS_VERSION,
        rep,
        0x00,
        ADDR_TYPE_IPV4,
        0x00, 0x00, 0x00, 0x00, // Bound address placeholder
        0x00, 0x00              // Bound port placeholder
    ];
    client_stream.write_all(&reply).await
}
