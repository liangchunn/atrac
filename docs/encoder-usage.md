# Encoder usage

Both codec crates expose the same streaming lifecycle at the crate root:

1. construct a validated profile;
2. create an encoder with an output writer and the input length per channel;
3. supply each requested deinterleaved PCM chunk;
4. consume the encoder with `finish` and recover the writer plus summary.

## Input and output contract

| Property | `at3` | `at3p` |
| --- | --- | --- |
| PCM sample type | signed 16-bit integer | signed 16-bit integer |
| Sample rate | 44,100 Hz | 44,100 Hz |
| Channel layout | one mono channel or two stereo channels | one mono channel or two stereo channels |
| Codec frame size | 1,024 sample frames | 2,048 sample frames |
| Minimum input | one sample frame per channel | 6,144 sample frames per channel |
| Output container | RIFF/WAVE with ATRAC3 format data | RIFF/WAVE ATRACX |

PCM is deinterleaved: `push_pcm(&[left, right])` supplies equal-length channel
slices. Do not assume that every requested chunk is a complete codec frame;
call `expected_next_chunk_frames()` before reading each chunk, especially for
the final partial chunk.

Both encoders perform their own priming and tail flushing. The input length in
the container describes the caller's original PCM, while the payload can
contain additional codec frames required by that schedule. Do not prepend or
append silence to compensate for codec delay.

`push_pcm_with_progress` and `finish_with_progress` expose optional progress
updates. `completed_steps / total_steps` includes priming and flushing work;
`completed_output_frames / total_output_frames` tracks payload frames. The
final `EncodeSummary` always reports `input_sample_frames`, `output_frames`,
`payload_bytes`, and `file_bytes`; ATRAC3 additionally reports encoded versus
silence-fallback frame counts.

WAV parsing and filesystem replacement policy are deliberately outside the
encoders. The CLI validates 16-bit integer PCM WAV input and atomically replaces
its destination only after the encoder and writer finish successfully.
