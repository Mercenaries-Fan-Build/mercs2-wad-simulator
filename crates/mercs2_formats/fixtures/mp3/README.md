# MP3 fixtures

Two tones made for `be_to_le::audio`'s tests; no game audio. Each is a raw MPEG-1 Layer III stream with no
Xing/LAME frame and no ID3 tag, the shape a PS3 wavebank clip has. Made with ffmpeg 9.0.1 and libmp3lame:

```
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=0.5" -ac 1 -c:a libmp3lame -b:a 64k \
  -write_xing 0 -id3v2_version 0 -fflags +bitexact -flags:a +bitexact tone_mono.mp3
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=0.5" \
  -f lavfi -i "sine=frequency=660:sample_rate=44100:duration=0.5" \
  -filter_complex "[0:a][1:a]join=inputs=2:channel_layout=stereo[a]" -map "[a]" \
  -c:a libmp3lame -b:a 128k -joint_stereo 1 -write_xing 0 -id3v2_version 0 \
  -fflags +bitexact -flags:a +bitexact tone_stereo.mp3
```

`tone_<x>.ffmpeg.s16le` is ffmpeg's decode of each, interleaved PCM16 little-endian, the reference the tests
hold the Rust decode within ±1 of:

```
ffmpeg -i tone_mono.mp3 -ac 1 -f s16le -acodec pcm_s16le tone_mono.ffmpeg.s16le
ffmpeg -i tone_stereo.mp3 -ac 2 -f s16le -acodec pcm_s16le tone_stereo.ffmpeg.s16le
```
