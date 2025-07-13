use std::env;

mod challenge00;
mod challenge01;
mod challenge02;
mod challenge03;
mod challenge04;
mod challenge05;
mod challenge06;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let challenge = &args[1];

    match challenge.as_str() {
        "0" => challenge00::run().unwrap(),
        "1" => challenge01::run().await?,
        "2" => challenge02::run().await?,
        "3" => challenge03::run().await?,
        "4" => challenge04::run().await?,
        "5" => challenge05::run().await.unwrap(),
        _ => panic!(),
    }

    Ok(())
}
