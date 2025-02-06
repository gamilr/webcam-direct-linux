use std::thread;

use anyhow::Result;
use gst::{
    glib::{self, MainLoop},
    prelude::*,
    Element, ElementFactory, Pipeline,
};
use gstreamer as gst;

fn create_pipeline(main_loop: MainLoop) -> Result<()> {
    gst::init()?;

    let pipeline = Pipeline::default();
    let src = ElementFactory::make("videotestsrc").build()?;
    let videoconv = ElementFactory::make("videoconvert").build()?;
    let sink = ElementFactory::make("autovideosink").build()?;

    pipeline.add_many([&src, &videoconv, &sink])?;

    Element::link_many(&[src, videoconv, sink])?;

    pipeline.set_state(gst::State::Playing)?;

    let bus = pipeline.bus().unwrap();

    let main_loop_clone = main_loop.clone();

    let _bus_watch = bus
        .add_watch(move |_, msg| {
            use gst::MessageView;

            let main_loop = &main_loop_clone;
            match msg.view() {
                MessageView::Eos(..) => {
                    println!("received eos");
                    // An EndOfStream event was sent to the pipeline, so we tell our main loop
                    // to stop execution here.
                    main_loop.quit()
                }
                MessageView::Error(err) => {
                    println!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    main_loop.quit();
                }
                _ => (),
            };

            // Tell the mainloop to continue executing this callback.
            glib::ControlFlow::Continue
        })
        .expect("Failed to add bus watch");

    println!("Running the main loop");
    main_loop.run();

    pipeline.set_state(gst::State::Null)?;

    println!("End of the main loop");

    Ok(())
}

#[tokio::main]
async fn main() {
    let main_loop = glib::MainLoop::new(None, false);

    let main_loop_clone = main_loop.clone();

    thread::spawn(move || match create_pipeline(main_loop_clone) {
        Ok(r) => r,
        Err(e) => eprintln!("Error! {e}"),
    });

    let mut seconds_to_die = 10;

    while seconds_to_die > 0 {
        println!(
            "Waiting for the pipeline to finish: {} seconds left",
            seconds_to_die
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
        seconds_to_die -= 1;
    }

    println!("Stopping the main loop from the main thread");
    main_loop.quit();
    println!("Main loop stopped");
}
