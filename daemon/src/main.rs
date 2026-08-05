use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const SOCKS_VERSION: u8 = 0x05;
const METHOD_NO_AUTH: u8 = 0x00;
const CMD_CONNECT: u8 = 0x01;

const ADDR_TYPE_IPV4: u8 = 0x01;
const ADDR_TYPE_DOMAIN: u8 = 0x03;
const ADDR_TYPE_IPV6: u8 = 0x04;

const REP_SUCCESS: u8 = 0x00;
const REP_HOST_UNREACHABLE: u8 = 0x04;
const REP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
const REP_ADDR_TYPE_NOT_SUPPORTED: u8 = 0x08;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listen_addr = "127.0.0.1:9999";
    let listener = TcpListener::bind(listen_addr).await?;
    println!("⚡ Sovereign daemon bound strictly to {}", listen_addr);
    println!("🛡️  Air-Gap Mode Active: Non-.anon traffic will be instantly dropped.");

    loop {
        let (stream, client_addr) = match listener.accept().await {
            Ok(val) => val,
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                continue;
            }
        };

        tokio::spawn(async move {
            if let Err(e) = handle_client(stream).await {
                eprintln!("Connection closed [{}]: {}", client_addr, e);
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

    let mut methods = vec![0u8; nmethods as usize];
    client_stream.read_exact(&mut methods).await?;

    if !methods.contains(&METHOD_NO_AUTH) {
        client_stream.write_all(&[SOCKS_VERSION, 0xFF]).await?;
        return Err("No acceptable authentication methods".into());
    }

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

    // 3. Parse Destination Host and Target String
    let (target_host, target_address) = match atyp {
        ADDR_TYPE_IPV4 => {
            let mut ipv4_bytes = [0u8; 4];
            client_stream.read_exact(&mut ipv4_bytes).await?;
            let mut port_bytes = [0u8; 2];
            client_stream.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            
            let ip_str = format!("{}.{}.{}.{}", ipv4_bytes[0], ipv4_bytes[1], ipv4_bytes[2], ipv4_bytes[3]);
            let full_addr = format!("{}:{}", ip_str, port);
            (ip_str, full_addr)
        }
        ADDR_TYPE_DOMAIN => {
            let len = client_stream.read_u8().await? as usize;
            let mut domain_bytes = vec![0u8; len];
            client_stream.read_exact(&mut domain_bytes).await?;
            let domain = String::from_utf8(domain_bytes)?;
            
            let mut port_bytes = [0u8; 2];
            client_stream.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);

            let full_addr = format!("{}:{}", domain, port);
            (domain, full_addr)
        }
        ADDR_TYPE_IPV6 => {
            let mut ipv6_bytes = [0u8; 16];
            client_stream.read_exact(&mut ipv6_bytes).await?;
            let mut port_bytes = [0u8; 2];
            client_stream.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);

            let full_addr = format!("[IPv6]:{}", port);
            ("ipv6".to_string(), full_addr)
        }
        _ => {
            send_reply(&mut client_stream, REP_ADDR_TYPE_NOT_SUPPORTED).await?;
            return Err("Address type not supported".into());
        }
    };

    // 4. Strict Domain Filtering & Enforcement
    let is_sovereign_domain = target_host.ends_with(".anon");

    if is_sovereign_domain {
        println!("⚡ [Sovereign Domain Resolved]: {}", target_address);
        
        // Acknowledge SOCKS5 connection success to browser
        send_reply(&mut client_stream, REP_SUCCESS).await?;
        
        // Read incoming HTTP request headers
        let mut browser_request = [0u8; 1024];
        let n = client_stream.read(&mut browser_request).await?;
        if n == 0 {
            return Ok(());
        }
        
        // Render mock P2P resource payload
        let mock_html = "HTTP/1.1 200 OK\r\n\
                         Content-Type: text/html; charset=utf-8\r\n\
                         Connection: close\r\n\r\n\
                         <!DOCTYPE html><html><head><title>Sovereign Web</title><style>body { background-color: #0f172a; color: #a855f7; font-family: system-ui, sans-serif; text-align: center; padding-top: 100px; } h1 { font-size: 2.5rem; } p { color: #94a3b8; }</style></head>\
                         <body><h1>⚡ Welcome to the Sovereign Web ⚡</h1><p>Rendering decentralized content from <strong>.anon</strong> cryptographic storage.</p></body></html>";
        
        client_stream.write_all(mock_html.as_bytes()).await?;
        client_stream.shutdown().await?;
        return Ok(());
    }

    // 5. AIR-GAP DROP: Immediately kill non-.anon clearnet attempts
    println!("🔒 [Clearnet Blocked]: {} (Access Denied)", target_address);
    send_reply(&mut client_stream, REP_HOST_UNREACHABLE).await?;

    Ok(())
}

async fn send_reply(client_stream: &mut TcpStream, rep: u8) -> Result<(), std::io::Error> {
    let reply = [
        SOCKS_VERSION,
        rep,
        0x00,
        ADDR_TYPE_IPV4,
        0x00, 0x00, 0x00, 0x00,
        0x00, 0x00
    ];
    client_stream.write_all(&reply).await
}
