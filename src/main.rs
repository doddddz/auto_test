use std::{sync::{mpsc, Arc, atomic}, process::{Command, Stdio}, path::PathBuf, io::Write};

use indicatif::{ProgressBar, ProgressStyle, ProgressState};
use structopt::StructOpt;
use tokio::sync::Semaphore;

pub type Res = Result<(), Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Res {
    let opt = Opt::from_args();
    println!("{:?}", &opt);
    let name = Arc::new(opt.test_part_name);
    let counter = Arc::new(Semaphore::new(opt.concurrency as _));
    let timestamp = chrono::offset::Local::now().timestamp();

    let (sender, receiver) = mpsc::channel::<TestRes>();

    let cor = tokio::spawn(async move {
        let bar = ProgressBar::new(opt.repeat_times);
        bar.set_style(ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] [{pos:>7}/{len:7}] {msg} {eta}"
        ).unwrap().with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));

        let mut pass = 0;
        let mut fail = 0;
        bar.set_position(0);
        for _ in 0..opt.repeat_times {
            let res = receiver.recv().unwrap();
            match res {
                TestRes::Pass => pass += 1,
                TestRes::Fail => fail += 1,
            }
            bar.set_message(format!("[PASS/FAIL] [{pass}/{fail}]"));
            bar.inc(1);
        }
        if pass == opt.repeat_times {
            bar.finish_with_message(format!("Congratulations, repeat {} times passed all of the test", opt.repeat_times))
        } else {
            bar.finish_with_message(format!("Passed: {pass}, Fail: {fail}"))
        }
    });

    let log_count_row = Arc::new(atomic::AtomicU32::new(0));

    for _ in 0..opt.repeat_times {
        let n = name.clone();
        let sd = sender.clone();
        let c = counter.clone();
        let log_count = log_count_row.clone();
        tokio::spawn(async move {
            let _permission = c.acquire_owned().await.unwrap();

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
                        std::fs::create_dir_all(format!("./log/{n}-{timestamp}")).unwrap();
                        let lc = log_count.fetch_add(1, atomic::Ordering::SeqCst);
                        let mut f = std::fs::OpenOptions::new()
                            .write(true)
                            .create(true)
                            .open(PathBuf::from(format!("./log/{n}-{timestamp}/fail-{lc}.log")))
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

#[derive(StructOpt, Debug, Clone)]
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
