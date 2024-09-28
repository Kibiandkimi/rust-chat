use log::debug;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

type IpList = Arc<Mutex<HashSet<String>>>;

#[tokio::main]
async fn main(){
    let args: Vec<String> = env::args().collect();

    let ip_list: IpList = Arc::new(Mutex::new(read_ip()));

    let listener = TcpListener::bind(&args[1]).await.unwrap();
    {
        let controller = TcpListener::bind(&args[2]).await.unwrap();
        let ip_list = ip_list.clone();
        tokio::spawn(async move {
            listen_controller(controller, ip_list).await;
        });
    }

    loop {
        let (socket, address) = listener.accept().await.unwrap();
        let ip_list = ip_list.clone();
        tokio::spawn(async move {
            process(socket, address, ip_list).await;
        });
    }

}

fn read_ip() -> HashSet<String> {
    debug!("Reading saved ip address from ip.ini.");

    let list = fs::read_to_string("ip.ini").unwrap();
    let mut ip_list = HashSet::new();
    for address in list.split("\n") {
        ip_list.insert(address.to_string());
    }

    ip_list
}

async fn process(mut socket: TcpStream, address: SocketAddr, ip_list: IpList) {
    debug!("Processing {address}.");

    let address = address.to_string();

    ip_list.lock().unwrap().insert(address.clone().to_string());

    let (reader, mut writer) = socket.split();

    let buf_reader = BufReader::new(reader);

    let mut line_iter = buf_reader.lines();

    let query_type = line_iter.next_line().await.unwrap().unwrap();

    match &query_type[..] {
        "oneshot" => {
            while let Some(message) = line_iter.next_line().await.unwrap() {
                output(message);
            }
        },
        "broadcast" => {
            let addr: Vec<_>;
            {
                let ip_list = ip_list.lock().unwrap();

                addr = ip_list.iter().filter(|adr| **adr != address).map(|adr| adr.clone()).collect(); // slowly ? move string ?
            }
            let mut message = String::new();
            while let Some(line) = line_iter.next_line().await.unwrap() {
                message += &line;
            }
            output(message.clone());

            message = "oneshot\n".to_string() + &message;

            for address in addr {
                send(&address, &message).await;
            }
        },
        "update" => {
            let address: Vec<_>;

            let mut message: String = String::new();
            {
                let ip_list = ip_list.lock().unwrap();

                address = ip_list.iter().collect(); // slowly ? move string ?

                for address in address {
                    message += &(address.clone() + "\n");
                }
            }
            writer.write_all(message.as_bytes()).await.unwrap();
        },
        _ => ()
    }
}

async fn send(address: &String, message: &String) {
    let mut stream = TcpStream::connect(&address).await.unwrap();
    stream.write_all(message.as_bytes()).await.unwrap();
}

fn output(message: String) {
    let mut stream = std::net::TcpStream::connect("localhost::7879").unwrap();
    stream.write(message.as_bytes()).unwrap();
}

async fn listen_controller(stream: TcpListener, ip_list: IpList) -> ! {

    while let Ok((mut socket, _)) = stream.accept().await {
        let (reader, _) = socket.split();
        let buf_reader = BufReader::new(reader);
        let mut lines = buf_reader.lines();
        let opt_type = lines.next_line().await.unwrap().unwrap();
        match &opt_type[..] {
            "oneshot" => {
                let address = lines.next_line().await.unwrap().unwrap();
                let mut message = String::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    message += &line;
                }
                message = "oneshot\n".to_string() + &message;
                send(&address, &message).await;
            },
            "broadcast" => {
                let addr: Vec<_>;
                {
                    let ip_list = ip_list.lock().unwrap();

                    addr = ip_list.iter().map(|adr| adr.clone()).collect(); // slowly ? move string ?
                }
                let mut message = String::new();
                while let Some(line) = lines.next_line().await.unwrap() {
                    message += &line;
                }

                message = "oneshot\n".to_string() + &message;

                for address in addr {
                    send(&address, &message).await;
                }
            },
            "update" => {
                let address = lines.next_line().await.unwrap().unwrap();
                let mut socket = TcpStream::connect(address).await.unwrap();
                let (reader, mut writer) = socket.split();
                writer.write_all("update\n".as_bytes()).await.unwrap();
                let buf_reader = BufReader::new(reader);
                let mut lines = buf_reader.lines();
                let mut new_list = Vec::new();
                while let Some(line) = lines.next_line().await.unwrap() {
                    new_list.push(line);
                }
                let mut ip_list = ip_list.lock().unwrap();
                for new_addr in new_list {
                    ip_list.insert(new_addr);
                }
            },
            _ => ()
        }
    }

    panic!("Listen controller end unexpectedly!");
}