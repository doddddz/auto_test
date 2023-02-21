use std::{sync::{mpsc, Arc}, process::{Command, Stdio}, io::Write};

use indicatif::{ProgressBar, ProgressStyle};
use structopt::StructOpt;
use tokio::sync::Semaphore;

pub type Res = Result<(), Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Res {
    let opt = Opt::from_args();
    println!("{:?}", &opt);
    let name = Arc::new(opt.test_part_name);
    let counter = Arc::new(Semaphore::new(opt.concurrency as _));

    let (sender, receiver) = mpsc::channel::<TestRes>();

    let cor = tokio::spawn(async move {
        let bar = ProgressBar::new(opt.repeat_times);
        bar.set_style(ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}"
        ).unwrap().progress_chars("##-"));

        let mut pass = 0;
        let mut fail = 0;
        for _ in 0..opt.repeat_times {
            let res = receiver.recv().unwrap();
            match res {
                TestRes::Pass => pass += 1,
                TestRes::Fail => fail += 1,
            }
            bar.set_message(format!("[PASS/FAIL] [{pass}/{fail}]"));
            bar.inc(1);
        }
    });

    for _ in 0..opt.repeat_times {
        let n = name.clone();
        let sd = sender.clone();
        let c = counter.clone();
        tokio::spawn(async move {
            let _ = c.acquire_owned().await.unwrap();

            let cmd = Command::new("go")
                .args(&["test", "--run", &n[..]])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap()
                .wait_with_output()
                .unwrap();

            if let Ok(s) = std::str::from_utf8(&cmd.stdout[..]) {
                let last_line = s.lines().last().unwrap();
                let status = *last_line.split(' ').collect::<Vec<&str>>().get(0).unwrap();
                let _ = sd.send(match status {
                    "ok" => TestRes::Pass,
                    _ => {
                        let timestamp = chrono::offset::Local::now();
                        let mut f = std::fs::OpenOptions::new()
                            .write(true)
                            .create(true)
                            .open(format!("err-{timestamp}.log"))
                            .unwrap();
                        f.write(s.as_bytes()).unwrap();
                        TestRes::Fail
                    },
                });
            }
        });
    }


    let _ = tokio::join!(cor);
    Ok(())
}

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(short, long, default_value = "500")]
    repeat_times: u64,
    #[structopt(short, default_value = "3")]
    concurrency: i32,
    #[structopt(short = "n", long, default_value = "2A")]
    test_part_name: String,
}

#[derive(Debug)]
pub enum TestRes {
    Pass,
    Fail,
}
