import init, {
    Atrac3Encoder,
    Atrac3plusEncoder,
    atrac3Bitrates,
    atrac3plusBitrates,
} from "./pkg/atrac_wasm.js";
import { parsePcmWave } from "./wav.js";

let working = false;

const initialized = init().then(() => {
    self.postMessage({
        type: "ready",
        profiles: {
            at3: {
                1: Array.from(atrac3Bitrates(1)),
                2: Array.from(atrac3Bitrates(2)),
            },
            at3p: {
                1: Array.from(atrac3plusBitrates(1)),
                2: Array.from(atrac3plusBitrates(2)),
            },
        },
    });
});

self.addEventListener("message", async (event) => {
    if (event.data?.type !== "encode") {
        return;
    }
    const { jobId, buffer, codec, bitrate, inputName } = event.data;
    if (working) {
        self.postMessage({ type: "error", jobId, message: "An encode is already running." });
        return;
    }
    working = true;
    try {
        await initialized;
        await encode({ jobId, buffer, codec, bitrate, inputName });
    } catch (error) {
        self.postMessage({
            type: "error",
            jobId,
            message: error instanceof Error ? error.message : String(error),
        });
    } finally {
        working = false;
    }
});

async function encode({ jobId, buffer, codec, bitrate, inputName }) {
    const info = parsePcmWave(buffer);
    const Encoder = codec === "at3" ? Atrac3Encoder : Atrac3plusEncoder;
    if (codec !== "at3" && codec !== "at3p") {
        throw new Error(`Unsupported codec ${codec}.`);
    }

    let encoder = null;
    const startedAt = performance.now();
    let lastProgressAt = Number.NEGATIVE_INFINITY;
    let lastPhase = null;
    try {
        encoder = new Encoder(bitrate, info.channels, info.sampleFrames);
        let pcmOffset = info.dataOffset;
        while (true) {
            const chunkFrames = encoder.expectedNextChunkFrames();
            if (chunkFrames === 0) {
                break;
            }
            const chunkBytes = chunkFrames * info.blockAlign;
            const pcm = new Uint8Array(buffer, pcmOffset, chunkBytes);
            const progress = encoder.pushPcm(pcm);
            try {
                const now = performance.now();
                const phaseChanged = progress.phase !== lastPhase;
                if (phaseChanged || now - lastProgressAt >= 50) {
                    postProgress(jobId, progress);
                    lastProgressAt = now;
                    lastPhase = progress.phase;
                }
            } finally {
                progress.free();
            }
            pcmOffset += chunkBytes;
        }
        if (pcmOffset !== info.dataOffset + info.dataSize) {
            throw new Error("Encoder did not consume the complete WAV data chunk.");
        }

        const beforeFinish = encoder.currentProgress();
        try {
            postProgress(jobId, beforeFinish, "flushing");
        } finally {
            beforeFinish.free();
        }
        const output = encoder.finish();
        const finalProgress = encoder.currentProgress();
        try {
            postProgress(jobId, finalProgress, "done", 100);
        } finally {
            finalProgress.free();
        }

        const outputBuffer =
            output.byteOffset === 0 && output.byteLength === output.buffer.byteLength
                ? output.buffer
                : output.slice().buffer;
        const sha256 = await sha256Hex(outputBuffer);
        self.postMessage(
            {
                type: "done",
                jobId,
                buffer: outputBuffer,
                fileName: outputFileName(inputName, codec, bitrate),
                elapsedMs: performance.now() - startedAt,
                outputBytes: output.byteLength,
                sha256,
            },
            [outputBuffer],
        );
    } finally {
        encoder?.free();
    }
}

async function sha256Hex(buffer) {
    const digest = await crypto.subtle.digest("SHA-256", buffer);
    return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function postProgress(jobId, progress, phase = progress.phase, forcedPercent = null) {
    const percent =
        forcedPercent ?? (progress.totalSteps > 0
            ? Math.min(100, (progress.completedSteps / progress.totalSteps) * 100)
            : 0);
    self.postMessage({
        type: "progress",
        jobId,
        phase,
        completedSteps: progress.completedSteps,
        totalSteps: progress.totalSteps,
        completedOutputFrames: progress.completedOutputFrames,
        totalOutputFrames: progress.totalOutputFrames,
        percent,
    });
}

function outputFileName(inputName, codec, bitrate) {
    const stem = inputName.replace(/\.wav$/i, "") || "output";
    const safeStem = stem.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_");
    return `${safeStem}-${codec}-${bitrate}kbps.at3`;
}
