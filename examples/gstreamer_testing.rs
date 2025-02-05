
use anyhow::Result;
use gst::{prelude::*, Element, ElementFactory, Pipeline};
use gstreamer as gst;
use log::error;

fn create_pipeline() -> Result<gst::Pipeline> {
    gst::init()?;

    let pipeline = Pipeline::default();
    let src = ElementFactory::make("videotestsrc").build()?;
    let videoconv = ElementFactory::make("videoconvert").build()?;
    let sink = ElementFactory::make("autovideosink").build()?;

    pipeline.add_many([&src, &videoconv, &sink])?;

    Element::link_many(&[src, videoconv, sink])?;

    Ok(pipeline)
}

fn main_loop(pipeline: gst::Pipeline) -> Result<()> {
    pipeline.set_state(gst::State::Playing)?;

    let bus = pipeline.bus().expect("Pipeline without bus. Shouldn't happen!");

    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(..) => break,
            MessageView::Error(err) => {
                pipeline.set_state(gst::State::Null)?;
                error!("Error {} in pipeline", err);
            }
            _ => (),
        }
    }

    pipeline.set_state(gst::State::Null)?;

    Ok(())
}

#[tokio::main]
async fn main() {
    tokio::spawn(async {
        match create_pipeline().and_then(main_loop) {
            Ok(r) => r,
            Err(e) => eprintln!("Error! {e}"),
        }
    });

    println!("Running the main loop");
}
