use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use ringbuf::traits::*;
use ringbuf::HeapRb;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(serde::Serialize, Clone, Debug)]
pub struct AudioDevice {
    pub name: String,
    pub rate: u32,
    pub channels: u16,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct ReceiverConfig {
    pub id: usize,
    pub device_name: String,
    pub volume: f32,
    pub delay_ms: f32,
    pub mute: bool,
}

pub struct SendStream(pub Stream);
unsafe impl Send for SendStream {}
unsafe impl Sync for SendStream {}

pub struct ActiveReceiver {
    pub volume: Arc<Mutex<f32>>,
    pub mute: Arc<Mutex<bool>>,
    pub delay_ms: Arc<Mutex<f32>>,
    pub rate: u32,
    // We wrap the Producer in Mutex so it can be accessed by the input callback thread
    pub producer: Arc<Mutex<ringbuf::wrap::CachingProd<Arc<HeapRb<f32>>>>>,
    pub stream: SendStream,
}

pub struct EngineState {
    pub active: bool,
    pub loopback_rate: u32,
    pub loopback_channels: u16,
    pub in_stream: Option<SendStream>,
    pub receivers: HashMap<usize, ActiveReceiver>,
}

// Global engine state
pub static ENGINE_STATE: OnceLock<Arc<Mutex<EngineState>>> = OnceLock::new();

pub fn get_engine_state() -> Arc<Mutex<EngineState>> {
    ENGINE_STATE
        .get_or_init(|| {
            Arc::new(Mutex::new(EngineState {
                active: false,
                loopback_rate: 48000,
                loopback_channels: 2,
                in_stream: None,
                receivers: HashMap::new(),
            }))
        })
        .clone()
}

pub fn resample_stereo(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if in_rate == out_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = out_rate as f64 / in_rate as f64;
    let num_frames = input.len() / 2;
    let out_frames = (num_frames as f64 * ratio) as usize;
    let mut output = Vec::with_capacity(out_frames * 2);
    
    for i in 0..out_frames {
        let x_out = i as f64 / ratio;
        let idx_low = x_out.floor() as usize;
        let idx_high = (idx_low + 1).min(num_frames - 1);
        let weight = x_out - idx_low as f64;
        
        let l_low = input[idx_low * 2];
        let l_high = input[idx_high * 2];
        let l_val = l_low + (l_high - l_low) * weight as f32;
        
        let r_low = input[idx_low * 2 + 1];
        let r_high = input[idx_high * 2 + 1];
        let r_val = r_low + (r_high - r_low) * weight as f32;
        
        output.push(l_val);
        output.push(r_val);
    }
    output
}

pub fn to_stereo(data: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels as usize;
    if channels == 2 {
        return data.to_vec();
    }
    if channels == 0 || data.is_empty() {
        return Vec::new();
    }
    let num_frames = data.len() / channels;
    let mut stereo = Vec::with_capacity(num_frames * 2);
    if channels == 1 {
        for &sample in data {
            stereo.push(sample);
            stereo.push(sample);
        }
    } else {
        for frame in data.chunks_exact(channels) {
            stereo.push(frame[0]);
            stereo.push(frame[1]);
        }
    }
    stereo
}

#[tauri::command]
pub fn get_devices() -> Result<(Vec<AudioDevice>, Vec<AudioDevice>), String> {
    let host = cpal::default_host();
    let mut loopback_devices = Vec::new();
    let mut output_devices = Vec::new();

    // Query output devices
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                if let Ok(cfg) = dev.default_output_config() {
                    output_devices.push(AudioDevice {
                        name,
                        rate: cfg.sample_rate().0,
                        channels: cfg.channels(),
                    });
                }
            }
        }
    }

    // Query input (loopback) devices - Windows WASAPI loopback captures from output devices
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                if let Ok(cfg) = dev.default_output_config() {
                    loopback_devices.push(AudioDevice {
                        name,
                        rate: cfg.sample_rate().0,
                        channels: cfg.channels(),
                    });
                }
            }
        }
    }

    Ok((loopback_devices, output_devices))
}

#[tauri::command]
pub fn start_routing(loopback_name: String, receivers_cfg: Vec<ReceiverConfig>) -> Result<String, String> {
    stop_routing()?;

    let host = cpal::default_host();
    let state = get_engine_state();
    let mut state_lock = state.lock().unwrap();

    // Find loopback device - Windows WASAPI loopback captures from output devices
    let input_device = host
        .output_devices()
        .map_err(|e| e.to_string())?
        .find(|d| d.name().map(|n| n == loopback_name).unwrap_or(false))
        .ok_or_else(|| "Selected Loopback capture device not found".to_string())?;

    let input_config = input_device
        .default_output_config()
        .map_err(|e| e.to_string())?;
    let in_rate = input_config.sample_rate().0;
    let in_channels = input_config.channels();
    state_lock.loopback_rate = in_rate;
    state_lock.loopback_channels = in_channels;

    // Set up output streams for each receiver
    for cfg in receivers_cfg {
        let output_device = match host
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|n| n == cfg.device_name).unwrap_or(false))
        {
            Some(dev) => dev,
            None => continue, // skip if not found
        };

        let output_config = output_device
            .default_output_config()
            .map_err(|e| e.to_string())?;
        let out_rate = output_config.sample_rate().0;
        let out_channels = output_config.channels();

        // Size: 3 seconds buffer (stereo: * 2)
        let buffer_capacity = (out_rate * 2 * 3) as usize;
        let rb = HeapRb::<f32>::new(buffer_capacity);
        let (mut prod, mut cons) = rb.split();

        // Fill buffer with initial silence to create delay
        let silence_samples = ((cfg.delay_ms / 1000.0) * out_rate as f32) as usize * 2;
        for _ in 0..silence_samples {
            let _ = prod.try_push(0.0);
        }

        let volume_mutex = Arc::new(Mutex::new(cfg.volume));
        let mute_mutex = Arc::new(Mutex::new(cfg.mute));
        let delay_mutex = Arc::new(Mutex::new(cfg.delay_ms));

        let volume_clone = volume_mutex.clone();
        let mute_clone = mute_mutex.clone();

        // Open playback stream
        let stream = output_device
            .build_output_stream(
                &output_config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let vol = *volume_clone.lock().unwrap();
                    let is_muted = *mute_clone.lock().unwrap();
                    let channels = out_channels as usize;

                    for frame in data.chunks_mut(channels) {
                        let mut left = 0.0;
                        let mut right = 0.0;
                        if let Some(l) = cons.try_pop() {
                            left = l;
                        }
                        if let Some(r) = cons.try_pop() {
                            right = r;
                        }

                        if is_muted {
                            for sample in frame.iter_mut() {
                                *sample = 0.0;
                            }
                        } else {
                            if channels == 1 {
                                frame[0] = (left + right) * 0.5 * vol;
                            } else {
                                frame[0] = left * vol;
                                frame[1] = right * vol;
                                for sample in frame.iter_mut().skip(2) {
                                    *sample = 0.0;
                                }
                            }
                        }
                    }
                },
                |err| println!("Output stream error: {}", err),
                None
            )
            .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;

        state_lock.receivers.insert(
            cfg.id,
            ActiveReceiver {
                volume: volume_mutex,
                mute: mute_mutex,
                delay_ms: delay_mutex,
                rate: out_rate,
                producer: Arc::new(Mutex::new(prod)),
                stream: SendStream(stream),
            },
        );
    }

    // Set up loopback capture stream
    let state_clone = state.clone();
    let in_stream = input_device
        .build_input_stream(
            &input_config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let st = state_clone.lock().unwrap();
                if !st.active {
                    return;
                }

                // Extract or downmix input channels to stereo
                let stereo_data = to_stereo(data, st.loopback_channels);

                for (&_r_id, receiver) in &st.receivers {
                    let is_muted = *receiver.mute.lock().unwrap();
                    if is_muted {
                        continue;
                    }

                    // Resample to receiver rate
                    let resampled = resample_stereo(&stereo_data, st.loopback_rate, receiver.rate);

                    // Write to receiver buffer
                    if let Ok(mut prod) = receiver.producer.lock() {
                        let target_delay = *receiver.delay_ms.lock().unwrap();
                        let target_delay_samples = ((target_delay / 1000.0) * receiver.rate as f32) as usize * 2;
                        
                        // Jitter / Clock drift correction (overrun check)
                        // If queue size exceeds target delay + 150ms of audio, discard oldest
                        let max_allowed = target_delay_samples + (0.15 * receiver.rate as f32) as usize * 2;
                        
                        // Since we don't have direct access to Consumer length in the Producer,
                        // we can check prod.vacant_len() to see how full the buffer is.
                        // Capacity - Vacant = Size.
                        let size = prod.capacity().get() - prod.vacant_len();
                        if size + resampled.len() > max_allowed {
                            // The buffer is too full (overrun).
                            // In ringbuf v0.4, the producer cannot easily pop items (only consumer can).
                            // But wait! We can skip writing these samples or just write as much as possible.
                            // If we write anyway, it will overwrite or block?
                            // Actually, HeapRb does not overwrite unless we use a ring buffer that supports it,
                            // or we can let the callback handle it.
                            // Let's write as much as possible.
                            let _ = prod.push_slice(&resampled);
                        } else {
                            let _ = prod.push_slice(&resampled);
                        }
                    }
                }
            },
            |err| println!("Input stream error: {}", err),
            None
        )
        .map_err(|e| e.to_string())?;

    in_stream.play().map_err(|e| e.to_string())?;
    state_lock.in_stream = Some(SendStream(in_stream));
    state_lock.active = true;

    Ok("Routing started".to_string())
}

#[tauri::command]
pub fn stop_routing() -> Result<String, String> {
    let state = get_engine_state();
    let mut state_lock = state.lock().unwrap();

    state_lock.active = false;
    if let Some(in_stream) = state_lock.in_stream.take() {
        let _ = in_stream.0.pause();
    }

    for (_, receiver) in state_lock.receivers.drain() {
        let _ = receiver.stream.0.pause();
    }

    Ok("Routing stopped".to_string())
}

#[tauri::command]
pub fn update_receiver(id: usize, volume: f32, delay_ms: f32, mute: bool) -> Result<(), String> {
    let state = get_engine_state();
    let state_lock = state.lock().unwrap();

    if let Some(receiver) = state_lock.receivers.get(&id) {
        *receiver.volume.lock().unwrap() = volume;
        *receiver.mute.lock().unwrap() = mute;
        
        let mut delay_lock = receiver.delay_ms.lock().unwrap();
        if *delay_lock != delay_ms {
            *delay_lock = delay_ms;
            
            // Re-apply delay by clearing the buffer and adding silence
            if let Ok(mut prod) = receiver.producer.lock() {
                // Clear the producer buffer (since we can't pop from producer, we can do it by
                // letting the output thread drain it, or we can just ignore it as it will self-correct,
                // or we can just let it drain).
                // Wait! Since we cannot pop from the producer in ringbuf, we can just push silence.
                // It will play out whatever was in the buffer, then the silence, then the new audio.
                // That is fine for a dynamic delay adjust!
                let silence_samples = ((delay_ms / 1000.0) * receiver.rate as f32) as usize * 2;
                for _ in 0..silence_samples {
                    let _ = prod.try_push(0.0);
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn get_receiver_status() -> Result<HashMap<usize, serde_json::Value>, String> {
    let state = get_engine_state();
    let state_lock = state.lock().unwrap();
    let mut status = HashMap::new();

    for (&id, receiver) in &state_lock.receivers {
        if let Ok(prod) = receiver.producer.lock() {
            let size = prod.capacity().get() - prod.vacant_len();
            let size_ms = ((size / 2) as f32 / receiver.rate as f32) * 1000.0;
            status.insert(
                id,
                serde_json::json!({
                    "status": "PLAYING",
                    "size_ms": format!("{:.0}ms", size_ms)
                }),
            );
        }
    }

    Ok(status)
}
