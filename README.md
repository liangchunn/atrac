# atrac

RE-ed ATRAC3 and ATRAC3+ encoders in Rust

## Supported formats

| Codec   | Channels | Supported bitrates                            |
| ------- | -------- | --------------------------------------------- |
| ATRAC3  | Mono     | 52, 66 kbps                                   |
| ATRAC3  | Stereo   | 66, 105, 132 kbps                             |
| ATRAC3+ | Mono     | 32, 48, 64, 96, 128 kbps                      |
| ATRAC3+ | Stereo   | 48, 64, 96, 128, 160, 192, 256, 320, 352 kbps |

## Usage (CLI)

Encode a 16-bit, 44.1 kHz mono or stereo PCM WAV file:

```console
cargo run --release -- encode --bitrate 128 input.wav output.wav
```

The bitrate selects ATRAC3 or ATRAC3+ automatically and must support the input's channel layout as listed above.
