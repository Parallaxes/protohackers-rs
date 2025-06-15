use std::env;

mod challenge01;
mod challenge02;
mod challenge03;
mod challenge04;
mod challenge04proto;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let challenge = &args[1];

    match challenge.as_str() {
        "1" => challenge01::run().unwrap(),
        "2" => challenge02::run().await?,
        "3" => challenge03::run().await?,
        "4" => challenge04::run().await?,
        "4p" => challenge04proto::run().await?,
        _ => panic!(),
    }

    Ok(())
}
