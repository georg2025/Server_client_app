use hyper::http::{Request};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use rand::Rng;
use std::{thread, time, io};
use tokio::sync::Semaphore;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use h2::server;
use http::{Response, StatusCode};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;



#[tokio::main]
async fn main() -> io::Result<()> {

    // Here we make a Listener to get incoming connections
    let addr: SocketAddr = ([127, 0, 0, 1], 3000).into();
    let tcp_listener = TcpListener::bind(addr).await?;

    // This are mutexes to count variables in async fn
    let client_number = Arc::new(Mutex::new(0));
    let clients_total = Arc::new(Mutex::new(0));
    let client_serve_time = Arc::new(Mutex::new(Vec::new()));
    let client_number_clone = Arc::clone(&client_number);
    let clients_total_clone = Arc::clone(&clients_total);
    let client_serve_time_clone = Arc::clone(&client_serve_time);

    // This code take place if we shut down server with ctrl-c
    tokio::task::spawn( async {
        tokio::signal::ctrl_c().await.expect("cant make ctrl_c signal");
        final_statistics(client_serve_time_clone, clients_total_clone,
        client_number_clone);
        std::process::exit(0);
        });


    // semaphore with 5 permits for  incomming messages
    let semaphore = Arc::new(Semaphore::new(5));

    loop {

        let (socket, _peer_addr) = tcp_listener.accept().await?;

        // we got one connection and we need to add 1 to our mutex
        let mut data = clients_total.lock().expect("unable to get data from mutex");
        *data+=1;

        // this are variables for our responce function
        let semaphore = Arc::clone(&semaphore);
        let client_number1 = Arc::clone(&client_number);
        let client_serve_time1 = Arc::clone(&client_serve_time);


        tokio::task::spawn (async move {
            if let Err(E) = responce(socket, semaphore, client_number1, client_serve_time1).await {
                eprintln!("Error with HttpConnection {}", E);
            };
        });
        }
}

// This function imitate searching for answer. Actually, it just sleep for 100-500 ms
// I left incomming variable request here, cause, maybe, it may be some changes
async fn answer_build (_request: Request<h2::RecvStream>) -> Response<()> {

        // Build a response
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(())
            .expect("cant build responce");

        let sleep_time = time::Duration::from_millis(rand::thread_rng().gen_range(100..=500));
        thread::sleep(sleep_time);
        return response

}

// This is where we responce to clients
async fn responce(socket: tokio::net::TcpStream, semaphore: Arc<tokio::sync::Semaphore>,
                client_number: Arc<Mutex<i32>>, client_serve_time: Arc<Mutex<Vec<time::Duration>>>) ->
                                   Result<(), Box<dyn std::error::Error + Send + Sync>>{
    // This is our mutex, where we count, how much time every single request took.
    let messages_time = Arc::new(Mutex::new(Vec::new()));
    let permit = semaphore.clone().acquire_owned().await?;
    let mut h2 = server::handshake(socket).await.expect("problems with handshake");

    // After handshake, we check, how much time is it (we need it to find out, how much time at all
    // we work with all requests of this client)
    let start = SystemTime::now();

    // Now, we accept single requests
    while let Some(request) = h2.accept().await {
        let message_time = Arc::clone(&messages_time);
        tokio::task::spawn (async move {
            // Now we need time to check, how much time we need for a single responce
        let start = SystemTime::now();

        let (request, mut respond) = request.expect("cant get request");
        println!("Received request: {:?}", request);
        // We make response with the help of fn above
        let response = answer_build(request).await;
        // Send respond
        respond.send_response(response, true).expect("cant send responce");
        // Now we stop the time and find out, how much did it take for single respond
        let stop = SystemTime::now();
        let difference = stop.duration_since(start).expect("We cant get time difference");
        // We push this time to our mutex vec
        let mut data = message_time.lock().expect("unable to get data from mutex");
        data.push(difference);
        println!("we give responce");
    });

    }
    // Here we find out, how much time the whole client serving take (all his requests)
    let stop = SystemTime::now();
    let difference = stop.duration_since(start).expect("we cant get time difference 2");
    // Here we count one more served client. Each time we end client serving - this mutex got +1
    let mut data = client_number.lock().expect("unable to get data from mutex");
    *data+=1;
    let number = *data;
    // Here we push our total client time to our mutex vec
    let mut data1 = client_serve_time.lock().expect("unable to get data from mutex");
    data1.push(difference);
    // And we protocole information on client statistics with the help of our fn
    client_statistic (Arc::clone(&messages_time), number, difference);
    drop(permit);
    Ok(())
}

// This function protocole our statistics about single client
fn client_statistic (messages_time: Arc<Mutex<Vec<time::Duration>>>, client_number: i32,
                    total_time: time::Duration) {

        // We make a file
    let srcdir = PathBuf::from(format!("client{}statistics.txt", client_number));
    let mut file = File::create(srcdir).expect("unable to create file");

    // First of all - if we have empty vec - that means, we didnt finished a single request.
    let messages_time1 = messages_time.lock().expect("Unable to open mutex");
    if messages_time1.len() == 0 {
        writeln!(file, "Ни одного клиентского запроса не было обработано")
            .expect("Проблемы с записью в файл 1");
    } else {

        // We set 3 variables: max, min time and summ of all requests time
        let mut total_requests_time = time::Duration::from_secs(0);
        let mut max_waiting_time = messages_time1[messages_time1.len()-1];
        let mut min_waiting_time = messages_time1[0];

        // We get summ of working times and look for min and max time
        for i in 0..messages_time1.len() {
            total_requests_time += messages_time1[i];
            if messages_time1[i]>max_waiting_time {
                max_waiting_time = messages_time1[i];
            }else if messages_time1[i]<min_waiting_time{
                min_waiting_time = messages_time1[i];
            }
        }
        // We got average time
        let average_request_time = total_requests_time/messages_time1.len() as u32;

        // We just need to write all the information.
        writeln!(file, "Общее количество запросов, на которые получены ответы: {:?}", messages_time1.len())
            .expect("Проблемы с записью в файл 2");
        writeln!(file, "Общее время работы с клиентом: {:?}", total_time)
            .expect("Проблемы с записью в файл 3");
        writeln!(file, "Минимальное время ответа на запрос клиента: {:?}", min_waiting_time)
            .expect("Проблемы с записью в файл 4");
        writeln!(file, "Среднее время ответа на запрос клиента: {:?}", average_request_time)
            .expect("Проблемы с записью в файл 5");
        writeln!(file, "Максимальное время ответа на запрос клиента: {:?}", max_waiting_time)
            .expect("Проблемы с записью в файл 6");
    }

}

fn final_statistics (clients_serve_time: Arc<Mutex<Vec<time::Duration>>>, clients_total: Arc<Mutex<i32>>,
                    client_number: Arc<Mutex<i32>>) {
    // Everything here is very similar to client_statistic fn. I will comment only if some difference occur
    let srcdir = PathBuf::from("total_statistics.txt");
    let mut file = File::create(srcdir).expect("unable to create file");

    let clients_serve_time = clients_serve_time.lock().expect("Unable to open mutex");
    if clients_serve_time.len() == 0 {
        writeln!(file, "За время работы не было отработанно ни одного клиента")
            .expect("Проблемы с записью в файл 1");
    } else {


        let mut total_requests_time = time::Duration::from_secs(0);
        let mut max_waiting_time = clients_serve_time[0];
        let mut min_waiting_time = clients_serve_time[0];


        for i in 0..clients_serve_time.len() {
            total_requests_time += clients_serve_time[i];
            if clients_serve_time[i]>max_waiting_time {
                max_waiting_time = clients_serve_time[i];
            }else if clients_serve_time[i]<min_waiting_time{
                min_waiting_time = clients_serve_time[i];
            }
        }

        let average_request_time = total_requests_time/clients_serve_time.len() as u32;
        // This is how we count clients, that hadnt been served. Total number of unserved client is
        // total incomming clients minus total served clients. Very simple
        let total = clients_total.lock().expect("Unable to open mutex");
        let number = client_number.lock().expect("Unable to open mutex");
        let hadnt_served = *total - *number;


        writeln!(file, "Общее количество проработанных клиентов: {:?}", *number)
            .expect("Проблемы с записью в файл 2");
        writeln!(file, "Минимальное время работы с клиентом: {:?}", min_waiting_time)
            .expect("Проблемы с записью в файл 4");
        writeln!(file, "Среднее время работы с клиентом: {:?}", average_request_time)
            .expect("Проблемы с записью в файл 5");
        writeln!(file, "Максимальное время работы с клиентом: {:?}", max_waiting_time)
            .expect("Проблемы с записью в файл 6");
        writeln!(file, "Количество клиентов, не дождавшихся обслуживания: {:?}", hadnt_served)
            .expect("Проблемы с записью в файл 6");
    }



}