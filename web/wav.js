const textDecoder = new TextDecoder("ascii");

function fourCc(bytes, offset) {
    return textDecoder.decode(bytes.subarray(offset, offset + 4));
}

function fail(message) {
    throw new Error(`Invalid WAV: ${message}`);
}

export function parsePcmWave(buffer) {
    const bytes = new Uint8Array(buffer);
    if (bytes.byteLength < 12) {
        fail("file is too short for a RIFF/WAVE header");
    }
    const view = new DataView(buffer);
    if (fourCc(bytes, 0) !== "RIFF") {
        fail("expected a RIFF header");
    }
    if (fourCc(bytes, 8) !== "WAVE") {
        fail("expected a WAVE form");
    }

    const declaredEnd = view.getUint32(4, true) + 8;
    if (declaredEnd > bytes.byteLength) {
        fail("declared RIFF size exceeds the file length");
    }

    let format = null;
    let data = null;
    let offset = 12;
    while (offset < declaredEnd && (!format || !data)) {
        if (declaredEnd - offset < 8) {
            fail(`chunk header at byte ${offset} is truncated`);
        }
        const id = fourCc(bytes, offset);
        const size = view.getUint32(offset + 4, true);
        const payloadOffset = offset + 8;
        const payloadEnd = payloadOffset + size;
        if (!Number.isSafeInteger(payloadEnd) || payloadEnd > declaredEnd) {
            fail(`${id || "unknown"} chunk at byte ${offset} is truncated`);
        }

        if (id === "fmt " && !format) {
            if (size < 16) {
                fail(`fmt chunk has ${size} bytes; expected at least 16`);
            }
            format = {
                formatTag: view.getUint16(payloadOffset, true),
                channels: view.getUint16(payloadOffset + 2, true),
                sampleRate: view.getUint32(payloadOffset + 4, true),
                averageBytesPerSecond: view.getUint32(payloadOffset + 8, true),
                blockAlign: view.getUint16(payloadOffset + 12, true),
                bitsPerSample: view.getUint16(payloadOffset + 14, true),
            };
        } else if (id === "data" && !data) {
            data = { offset: payloadOffset, size };
        }

        offset = payloadEnd + (size & 1);
    }

    if (!format) {
        fail("missing fmt chunk");
    }
    if (!data) {
        fail("missing data chunk");
    }
    if (format.formatTag !== 1) {
        fail(`format tag ${format.formatTag} is not integer PCM`);
    }
    if (format.channels !== 1 && format.channels !== 2) {
        fail(`expected mono or stereo PCM, got ${format.channels} channels`);
    }
    if (format.sampleRate !== 44_100) {
        fail(`sample rate must be 44100 Hz, got ${format.sampleRate}`);
    }
    if (format.bitsPerSample !== 16) {
        fail(`sample depth must be 16-bit, got ${format.bitsPerSample}`);
    }
    const expectedBlockAlign = format.channels * 2;
    if (format.blockAlign !== expectedBlockAlign) {
        fail(`block alignment must be ${expectedBlockAlign}, got ${format.blockAlign}`);
    }
    if (data.size % format.blockAlign !== 0) {
        fail(`data size ${data.size} is not aligned to ${format.blockAlign}-byte frames`);
    }
    const sampleFrames = data.size / format.blockAlign;
    if (sampleFrames === 0) {
        fail("PCM data is empty");
    }
    if (sampleFrames > 0xffff_ffff) {
        fail("PCM stream has more frames than the encoder can represent");
    }

    return {
        ...format,
        dataOffset: data.offset,
        dataSize: data.size,
        sampleFrames,
        durationSeconds: sampleFrames / format.sampleRate,
    };
}

export function formatDuration(seconds) {
    const totalSeconds = Math.round(seconds);
    const minutes = Math.floor(totalSeconds / 60);
    const remainder = totalSeconds % 60;
    return `${minutes}:${String(remainder).padStart(2, "0")}`;
}
