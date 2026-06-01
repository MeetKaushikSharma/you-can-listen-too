// YOUCANLISTENTOO Frontend Controller

const { invoke } = window.__TAURI__.core;

// State management
let isRouting = false;
let loopbackDevices = [];
let outputDevices = [];
let statusInterval = null;

// Receivers states
const receivers = Array.from({ length: 5 }, (_, i) => ({
  id: i,
  enabled: false,
  deviceName: "",
  volume: 1.0,
  delayMs: 0.0,
  mute: false,
}));

// DOM Elements
const loopbackSelect = document.getElementById("loopback-select");
const toggleBtn = document.getElementById("toggle-routing-btn");
const refreshBtn = document.getElementById("refresh-devices-btn");
const systemStatusBadge = document.getElementById("system-status");

// Select elements for each receiver slot
const slots = Array.from({ length: 5 }, (_, i) => {
  const el = document.querySelector(`.receiver-slot[data-id="${i}"]`);
  return {
    id: i,
    el,
    enableChk: el.querySelector(".receiver-enable"),
    statusText: el.querySelector(".receiver-status"),
    deviceSelect: el.querySelector(".receiver-device"),
    delaySlider: el.querySelector(".receiver-delay"),
    delayLabel: el.querySelector(".delay-val"),
    volSlider: el.querySelector(".receiver-volume"),
    volLabel: el.querySelector(".vol-val"),
    muteBtn: el.querySelector(".receiver-mute-btn"),
  };
});

// 1. Initial setup
document.addEventListener("DOMContentLoaded", () => {
  setupEventListeners();
  refreshDevices();
});

function setupEventListeners() {
  refreshBtn.addEventListener("click", refreshDevices);
  toggleBtn.addEventListener("click", toggleRouting);

  // Setup credit link event listener to open LinkedIn url in external browser
  const creditsLink = document.getElementById("credits-link");
  if (creditsLink) {
    creditsLink.addEventListener("click", async (e) => {
      e.preventDefault();
      try {
        if (window.__TAURI__ && window.__TAURI__.opener) {
          await window.__TAURI__.opener.openUrl("https://www.linkedin.com/in/meetkaushiksharma/");
        }
      } catch (err) {
        console.error("Failed to open URL:", err);
      }
    });
  }

  // Setup event listeners for each receiver slot
  slots.forEach((slot) => {
    slot.enableChk.addEventListener("change", (e) => {
      receivers[slot.id].enabled = e.target.checked;
      syncDynamicSettings(slot.id);
    });

    slot.deviceSelect.addEventListener("change", (e) => {
      receivers[slot.id].deviceName = e.target.value;
    });

    slot.delaySlider.addEventListener("input", (e) => {
      const val = parseFloat(e.target.value);
      slot.delayLabel.textContent = `DELAY: ${val}ms`;
      receivers[slot.id].delayMs = val;
      syncDynamicSettings(slot.id);
    });

    slot.volSlider.addEventListener("input", (e) => {
      const val = parseInt(e.target.value);
      slot.volLabel.textContent = `VOL: ${val}%`;
      receivers[slot.id].volume = val / 100.0;
      syncDynamicSettings(slot.id);
    });

    slot.muteBtn.addEventListener("click", () => {
      receivers[slot.id].mute = !receivers[slot.id].mute;
      if (receivers[slot.id].mute) {
        slot.muteBtn.classList.add("mute-active");
        slot.muteBtn.textContent = "MUTED";
      } else {
        slot.muteBtn.classList.remove("mute-active");
        slot.muteBtn.textContent = "MUTE";
      }
      syncDynamicSettings(slot.id);
    });
  });
}

// 2. Fetch sound devices
async function refreshDevices() {
  try {
    const [loopbacks, outputs] = await invoke("get_devices");
    loopbackDevices = loopbacks;
    outputDevices = outputs;

    // Populate Input Loopback Dropdown
    const currentLoopback = loopbackSelect.value;
    loopbackSelect.innerHTML = `<option disabled selected>[SELECT SOURCE]</option>`;
    
    let defaultSelection = "";
    loopbacks.forEach((dev) => {
      const opt = document.createElement("option");
      opt.value = dev.name;
      opt.textContent = dev.name;
      loopbackSelect.appendChild(opt);
      
      // Auto-select VB-Cable or first loopback
      if (dev.name.toLowerCase().includes("cable") || dev.name.toLowerCase().includes("loopback") && dev.name.toLowerCase().includes("speakers")) {
        defaultSelection = dev.name;
      }
    });

    if (loopbacks.length > 0) {
      if (loopbacks.some(d => d.name === currentLoopback)) {
        loopbackSelect.value = currentLoopback;
      } else if (defaultSelection) {
        loopbackSelect.value = defaultSelection;
      } else {
        loopbackSelect.value = loopbacks[0].name;
      }
    }

    // Populate Output Dropdowns for all receivers
    slots.forEach((slot) => {
      const currentDev = slot.deviceSelect.value;
      slot.deviceSelect.innerHTML = `<option disabled selected>[SELECT DEVICE]</option>`;
      
      outputs.forEach((dev) => {
        const opt = document.createElement("option");
        opt.value = dev.name;
        opt.textContent = dev.name;
        slot.deviceSelect.appendChild(opt);
      });

      if (outputs.some(d => d.name === currentDev)) {
        slot.deviceSelect.value = currentDev;
      }
    });

  } catch (err) {
    alert("Failed to enumerate audio devices: " + err);
  }
}

// 3. Dynamic adjustment updates
async function syncDynamicSettings(id) {
  if (!isRouting) return;
  const receiver = receivers[id];
  try {
    await invoke("update_receiver", {
      id: receiver.id,
      volume: receiver.volume,
      delayMs: receiver.delayMs,
      // If receiver is disabled, we tell the engine to mute it
      mute: receiver.mute || !receiver.enabled,
    });
  } catch (err) {
    console.error("Failed to update receiver settings:", err);
  }
}

// 4. Start/Stop Master Routing
async function toggleRouting() {
  if (isRouting) {
    // Stop routing
    try {
      await invoke("stop_routing");
      isRouting = false;
      
      // UI Reset
      toggleBtn.textContent = "START ROUTING";
      systemStatusBadge.textContent = "● SYSTEM OFFLINE";
      systemStatusBadge.style.backgroundColor = "#000000";
      systemStatusBadge.style.color = "#ffffff";
      
      slots.forEach((slot) => {
        slot.statusText.textContent = "OFFLINE";
        slot.statusText.style.color = "var(--text-dim)";
      });

      clearInterval(statusInterval);
      statusInterval = null;
    } catch (err) {
      alert("Failed to stop routing: " + err);
    }
  } else {
    // Start routing
    const loopbackName = loopbackSelect.value;
    if (!loopbackName || loopbackName.startsWith("[")) {
      alert("Please select a valid capture source.");
      return;
    }

    // Filter and prepare active configs
    const activeConfigs = [];
    let enabledCount = 0;

    for (const r of receivers) {
      if (r.enabled) {
        if (!r.deviceName || r.deviceName.startsWith("[")) {
          alert(`Receiver 0${r.id + 1} is enabled but has no output device selected.`);
          return;
        }
        activeConfigs.push({
          id: r.id,
          device_name: r.deviceName,
          volume: r.volume,
          delay_ms: r.delayMs,
          mute: r.mute,
        });
        enabledCount++;
      }
    }

    if (enabledCount === 0) {
      alert("Please enable at least one receiver checkbox.");
      return;
    }

    try {
      await invoke("start_routing", {
        loopbackName,
        receiversCfg: activeConfigs,
      });

      isRouting = true;
      toggleBtn.textContent = "STOP ROUTING";
      systemStatusBadge.textContent = "● ROUTING ACTIVE";
      systemStatusBadge.style.backgroundColor = "#ffffff";
      systemStatusBadge.style.color = "#000000";

      // Start Polling
      statusInterval = setInterval(pollReceiverStatus, 250);
    } catch (err) {
      alert("Engine Start Error: " + err);
    }
  }
}

// 5. Polling stats
async function pollReceiverStatus() {
  if (!isRouting) return;
  try {
    const statusMap = await invoke("get_receiver_status");
    slots.forEach((slot) => {
      const stats = statusMap[slot.id];
      if (stats && receivers[slot.id].enabled) {
        slot.statusText.textContent = `PLAYING | BUF: ${stats.size_ms}`;
        slot.statusText.style.color = "var(--text-color)";
      } else if (receivers[slot.id].enabled) {
        slot.statusText.textContent = "STARTING";
        slot.statusText.style.color = "var(--text-dim)";
      } else {
        slot.statusText.textContent = "OFFLINE";
        slot.statusText.style.color = "var(--text-dim)";
      }
    });
  } catch (err) {
    console.error("Error polling receiver statuses:", err);
  }
}
