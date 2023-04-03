use std::path::PathBuf;

use {opencv::core, std::env, std::time::SystemTime};
pub mod common;
mod par_better;
mod par_rust_ssp;
mod par_std_threads;
mod par_tokio;
mod seq;
mod spar_rust;

pub static mut FACE_XML_STR: String = String::new();
pub static mut EYE_XML_STR: String = String::new();

fn main() -> opencv::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!();
        panic!(
            "Correct usage: $ ./{:?} <run_mode> <nthreads> <input_video>",
            args[0]
        );
    }

    // For our analysis, we don't want OpenCV's parallelism
    core::set_num_threads(1).unwrap();

    // Arguments
    let run_mode = &args[1];
    let nthreads = args[2].parse::<i32>().unwrap();
    let input_video = &args[3];

    let mut dir = PathBuf::from(input_video);
    dir.pop();
    dir.push("haarcascade_frontalface_alt.xml");
    unsafe {
        FACE_XML_STR = dir.to_string_lossy().to_string();
    }
    dir.pop();
    dir.push("haarcascade_eye.xml");
    unsafe {
        EYE_XML_STR = dir.to_string_lossy().to_string();
    }

    let start = SystemTime::now();

    match run_mode.as_str() {
        "seq" => seq::seq_eye_tracker(input_video)?,
        "rust-ssp" => par_rust_ssp::rust_ssp_eye_tracker(input_video, nthreads)?,
        "tokio" => par_tokio::tokio_eye_tracker(input_video, nthreads)?,
        "std-threads" => par_std_threads::std_threads_eye_tracker(input_video, nthreads)?,
        "better" => par_better::better_eye_tracker(input_video, nthreads)?,
        "spar-rust" => spar_rust::spar_rust_eye_tracker(input_video, nthreads)?,
        _ => println!(
            "Invalid run_mode, use (seq | rust-ssp | tokio | std-threads | better | spar-rust)"
        ),
    }

    let system_duration = start.elapsed().expect("Failed to get render time?");
    let in_sec = system_duration.as_secs() as f64 + system_duration.subsec_nanos() as f64 * 1e-9;
    println!("Execution time: {in_sec} sec");
    Ok(())
}
