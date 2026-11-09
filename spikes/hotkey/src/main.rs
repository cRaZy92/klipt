// Spike C6: register a global shortcut through the XDG GlobalShortcuts portal and print activations.
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let portal = GlobalShortcuts::new().await?;
    let session = portal.create_session().await?;
    let shortcut = NewShortcut::new("save-clip", "Klipt: save clip").preferred_trigger(Some("ALT+F9"));
    let bound = portal.bind_shortcuts(&session, &[shortcut], None).await?.response()?;
    for s in bound.shortcuts() {
        println!("bound: id={} trigger={}", s.id(), s.trigger_description());
    }
    let mut activated = portal.receive_activated().await?;
    println!("listening — press the shortcut (Ctrl+C to stop)");
    let t0 = std::time::Instant::now();
    while let Some(ev) = activated.next().await {
        println!("ACTIVATED id={} at +{:.1}s", ev.shortcut_id(), t0.elapsed().as_secs_f32());
    }
    Ok(())
}
