import { formatDuration, parsePcmWave } from "./wav.js";

const elements = {
    form: document.querySelector("#encode-form"),
    file: document.querySelector("#wav-file"),
    codec: document.querySelector("#codec"),
    bitrate: document.querySelector("#bitrate"),
    button: document.querySelector("#encode-button"),
    metadata: document.querySelector("#metadata"),
    metaName: document.querySelector("#meta-name"),
    metaDuration: document.querySelector("#meta-duration"),
    metaFormat: document.querySelector("#meta-format"),
    metaFrames: document.querySelector("#meta-frames"),
    wasmStatus: document.querySelector("#wasm-status"),
    progress: document.querySelector("#progress"),
    progressPercent: document.querySelector("#progress-percent"),
    jobStatus: document.querySelector("#job-status"),
    frameStatus: document.querySelector("#frame-status"),
    checksum: document.querySelector("#checksum"),
    downloadAgain: document.querySelector("#download-again"),
    error: document.querySelector("#error"),
};

const worker = new Worker(new URL("./encoder-worker.js", import.meta.url), { type: "module" });
let profiles = null;
let selectedFile = null;
let selectedInfo = null;
let busy = false;
let nextJobId = 1;
let activeJobId = null;
let downloadUrl = null;

worker.addEventListener("message", (event) => {
    const message = event.data;
    if (message.type === "ready") {
        profiles = message.profiles;
        elements.wasmStatus.textContent = "Encoder ready";
        elements.wasmStatus.classList.add("ready");
        updateControls();
        return;
    }
    if (message.jobId !== activeJobId) {
        return;
    }
    if (message.type === "progress") {
        renderProgress(message);
    } else if (message.type === "done") {
        finishJob(message);
    } else if (message.type === "error") {
        failJob(message.message);
    }
});

worker.addEventListener("error", (event) => {
    elements.wasmStatus.textContent = "Encoder failed to load";
    failJob(event.message || "The encoder worker failed.");
});

elements.file.addEventListener("change", async () => {
    clearError();
    selectedFile = elements.file.files?.[0] ?? null;
    selectedInfo = null;
    elements.metadata.hidden = true;
    elements.downloadAgain.hidden = true;
    if (!selectedFile) {
        elements.jobStatus.textContent = "Waiting for a valid WAV.";
        updateControls();
        return;
    }

    elements.jobStatus.textContent = "Inspecting WAV…";
    try {
        selectedInfo = parsePcmWave(await selectedFile.arrayBuffer());
        renderMetadata();
        updateBitrates();
        elements.jobStatus.textContent = "Ready to encode.";
    } catch (error) {
        showError(error instanceof Error ? error.message : String(error));
        elements.jobStatus.textContent = "Choose a supported WAV to continue.";
    }
    updateControls();
});

elements.codec.addEventListener("change", () => {
    clearError();
    updateBitrates();
    updateControls();
});

elements.form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!selectedFile || !selectedInfo || !profiles || busy) {
        return;
    }
    if (elements.codec.value === "at3p" && selectedInfo.sampleFrames < 6144) {
        showError("ATRAC3plus requires at least 6144 PCM frames per channel.");
        return;
    }

    clearError();
    resetDownload();
    setBusy(true);
    renderProgress({
        phase: "loading",
        percent: 0,
        completedOutputFrames: 0,
        totalOutputFrames: 0,
    });
    activeJobId = nextJobId++;
    try {
        const buffer = await selectedFile.arrayBuffer();
        worker.postMessage(
            {
                type: "encode",
                jobId: activeJobId,
                buffer,
                codec: elements.codec.value,
                bitrate: Number(elements.bitrate.value),
                inputName: selectedFile.name,
            },
            [buffer],
        );
    } catch (error) {
        failJob(error instanceof Error ? error.message : String(error));
    }
});

loadLocalFixture();

async function loadLocalFixture() {
    const fixture = new URL(window.location.href).searchParams.get("fixture");
    if (!fixture || !["127.0.0.1", "localhost"].includes(window.location.hostname)) {
        return;
    }
    clearError();
    elements.jobStatus.textContent = "Loading local browser-test fixture…";
    try {
        const response = await fetch(fixture);
        if (!response.ok) {
            throw new Error(`Fixture request failed with HTTP ${response.status}.`);
        }
        const blob = await response.blob();
        const name = fixture.split("/").filter(Boolean).at(-1) || "fixture.wav";
        selectedFile = new File([blob], name, { type: blob.type || "audio/wav" });
        selectedInfo = parsePcmWave(await selectedFile.arrayBuffer());
        renderMetadata();
        updateBitrates();
        elements.jobStatus.textContent = "Local fixture ready to encode.";
        updateControls();
    } catch (error) {
        selectedFile = null;
        selectedInfo = null;
        failJob(error instanceof Error ? error.message : String(error));
    }
}

function renderMetadata() {
    elements.metadata.hidden = false;
    elements.metaName.textContent = selectedFile.name;
    elements.metaName.title = selectedFile.name;
    elements.metaDuration.textContent = formatDuration(selectedInfo.durationSeconds);
    elements.metaFormat.textContent = `${selectedInfo.channels === 1 ? "Mono" : "Stereo"} · ${selectedInfo.sampleRate.toLocaleString()} Hz · ${selectedInfo.bitsPerSample}-bit`;
    elements.metaFrames.textContent = selectedInfo.sampleFrames.toLocaleString();
}

function updateBitrates() {
    const previous = Number(elements.bitrate.value);
    elements.bitrate.replaceChildren();
    if (!profiles || !selectedInfo) {
        return;
    }
    const rates = profiles[elements.codec.value]?.[selectedInfo.channels] ?? [];
    for (const rate of rates) {
        const option = document.createElement("option");
        option.value = String(rate);
        option.textContent = `${rate} kbps`;
        elements.bitrate.append(option);
    }
    if (rates.includes(previous)) {
        elements.bitrate.value = String(previous);
    } else if (rates.length > 0) {
        elements.bitrate.value = String(rates.at(-1));
    }
}

function updateControls() {
    if (profiles && selectedInfo && elements.bitrate.options.length === 0) {
        updateBitrates();
    }
    const enabled = Boolean(profiles && selectedInfo && !busy);
    elements.file.disabled = busy;
    elements.codec.disabled = !enabled;
    elements.bitrate.disabled = !enabled;
    elements.button.disabled = !enabled;
}

function setBusy(value) {
    busy = value;
    elements.button.textContent = value ? "Encoding…" : "Encode and download";
    updateControls();
}

function renderProgress(message) {
    const percent = message.percent == null ? elements.progress.value : message.percent;
    elements.progress.value = percent;
    elements.progress.textContent = `${Math.round(percent)}%`;
    elements.progressPercent.value = `${Math.round(percent)}%`;
    const phaseLabels = {
        loading: "Loading WAV…",
        preparing: "Preparing encoder…",
        encoding: "Encoding audio…",
        flushing: "Flushing codec tail…",
        done: "Encode complete.",
    };
    elements.jobStatus.textContent = phaseLabels[message.phase] ?? "Encoding audio…";
    const completed = message.completedOutputFrames ?? 0;
    const total = message.totalOutputFrames ?? 0;
    elements.frameStatus.textContent = `${completed.toLocaleString()} / ${total.toLocaleString()} frames`;
}

function finishJob(message) {
    const bytes = new Uint8Array(message.buffer);
    downloadUrl = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
    elements.downloadAgain.href = downloadUrl;
    elements.downloadAgain.download = message.fileName;
    elements.downloadAgain.dataset.sha256 = message.sha256;
    elements.downloadAgain.dataset.outputBytes = String(message.outputBytes);
    elements.downloadAgain.hidden = false;
    elements.downloadAgain.click();
    elements.checksum.textContent = `SHA-256 ${message.sha256}`;
    elements.checksum.hidden = false;
    elements.progress.value = 100;
    elements.progressPercent.value = "100%";
    elements.jobStatus.textContent = `Done in ${(message.elapsedMs / 1000).toFixed(2)} s · ${formatBytes(message.outputBytes)} downloaded.`;
    setBusy(false);
    activeJobId = null;
}

function failJob(message) {
    showError(message);
    elements.jobStatus.textContent = "Encoding failed.";
    setBusy(false);
    activeJobId = null;
}

function resetDownload() {
    if (downloadUrl) {
        URL.revokeObjectURL(downloadUrl);
        downloadUrl = null;
    }
    elements.downloadAgain.hidden = true;
    elements.downloadAgain.removeAttribute("href");
    delete elements.downloadAgain.dataset.sha256;
    delete elements.downloadAgain.dataset.outputBytes;
    elements.checksum.hidden = true;
    elements.checksum.textContent = "";
}

function showError(message) {
    elements.error.textContent = message;
    elements.error.hidden = false;
}

function clearError() {
    elements.error.hidden = true;
    elements.error.textContent = "";
}

function formatBytes(bytes) {
    if (bytes < 1024) {
        return `${bytes} B`;
    }
    if (bytes < 1024 * 1024) {
        return `${(bytes / 1024).toFixed(1)} KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}
