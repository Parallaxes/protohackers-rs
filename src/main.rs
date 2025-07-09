use std::env;

mod challenge00;
mod challenge01;
mod challenge03;
mod challenge04;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let challenge = &args[1];

    match challenge.as_str() {
        "1" => challenge00::run().unwrap(),
        "2" => challenge01::run().await?,
        "3" => challenge03::run().await?,
        "4" => challenge04::run().await?,
        "4p" => challenge03::run().await?,
        "5" => challenge04::run().await?,
        _ => panic!(),
    }

    Ok(())
}
