//! GPU time per render pass, for measuring what lighting techniques cost.
//!
//! `DIORAMA_GPU_TIMES=1 cargo run --release` logs every few seconds how long each render pass took
//! on the GPU (averaged over the last frames), plus their sum. Needs timestamp queries (Vulkan or
//! DX12); elsewhere only CPU times get recorded and nothing is logged. Bevy doesn't time every
//! pass: shadow maps and volumetric fog are missing from the list. (Timing them with extra spans
//! around Bevy's passes hung the app on exit, so don't.)

use bevy::{diagnostic::DiagnosticsStore, prelude::*, render::diagnostic::RenderDiagnosticsPlugin};

/// Real seconds between reports.
const REPORT_INTERVAL: f32 = 5.0;
/// Passes quicker than this (ms) are left out of the list, but still count toward the sum.
const SHOW_ABOVE_MS: f64 = 0.02;

pub struct GpuTimesPlugin;

impl Plugin for GpuTimesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RenderDiagnosticsPlugin)
            .add_systems(Update, report);
    }
}

fn report(
    time: Res<Time<Real>>,
    store: Res<DiagnosticsStore>,
    mut since: Local<f32>,
    mut frames: Local<u32>,
) {
    *since += time.delta_secs();
    *frames += 1;
    if *since < REPORT_INTERVAL {
        return;
    }
    let frame_ms = *since * 1000.0 / *frames as f32;
    (*since, *frames) = (0.0, 0);
    let mut passes: Vec<(String, f64)> = store
        .iter()
        .filter_map(|diagnostic| {
            let path = diagnostic.path().as_str();
            let pass = path.strip_prefix("render/")?.strip_suffix("/elapsed_gpu")?;
            Some((pass.to_string(), diagnostic.average()?))
        })
        .collect();
    if passes.is_empty() {
        return;
    }
    passes.sort_by(|a, b| a.0.cmp(&b.0));
    // Nested passes are already counted in their parent's time.
    let total: f64 = passes
        .iter()
        .filter(|(pass, _)| !pass.contains('/'))
        .map(|(_, ms)| ms)
        .sum();
    let mut lines = vec![format!(
        "GPU {total:.2} ms in timed passes, frame {frame_ms:.2} ms"
    )];
    lines.extend(
        passes
            .iter()
            .filter(|(_, ms)| *ms >= SHOW_ABOVE_MS)
            .map(|(pass, ms)| format!("  {ms:>6.3}  {pass}")),
    );
    info!("{}", lines.join("\n"));
}
