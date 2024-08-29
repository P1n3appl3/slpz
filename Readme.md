# SLP Compress

Compresses and decompresses between the slp and slpz formats.

You can expect slpz files to be around 8x to 12x times smaller than slp files for regular matches.
(~3Mb down to ~300Kb).

Compression is done with the zstd compression library. 
zstd is not required on the user's computer; the library is statically linked at compile time.

# Comparison with 'slippc'
slippc is very impressive. 
They have achieved much higher compression rates by abusing the contents of events.
However, in my opinion, this comes with three big drawbacks:
1. *Maintentance*: Due to abusing the structure of events, slippc is beholden to the slp spec and must be manually updated for each version.
Slippc has not been updated for over a year and fails on new replays.
slp_compress does not care about the contents of events (Other than the Event Payloads event). 
It will work for all slp spec changes in the future.
2. *Complexity*: slippc is over 10000 lines of code. slp_compress achieves comparable compression with 300 lines of code. 
This is not entirely a fair comparison as slippc does more than compress replays, but the point stands.
slp_compress is much easier to understand and modify than slippc.
3. *Performance*: slp_compress uses zstd compression. slippc uses lzma compression.
lzma compression is better than zstd, but takes order of magnitudes longer to compress and decompress.

# The SLPZ Format

## Header
24 bytes.
- 0..4: Version. Current version is 0
- 4..8: Event Sizes offset
- 8..12: Game Start offset
- 12..16: Metadata offset
- 16..20: Compressed events offset
- 20..24: size of uncompressed events

All offsets are from file start.

## Event Sizes
This is equivalent the 'Event Payloads' event in the [SLP Spec](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#event-payloads).

## Game start
This is equivalent the 'Game Start' event in the [SLP Spec](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#game-start).

## Metadata
This is equivalent the 'Metadata' event in the [SLP Spec](https://github.com/project-slippi/slippi-wiki/blob/master/SPEC.md#the-metadata-element).

## Compressed Events
This is reordered events passed through zstd compression.

# Event Reordering?
Reordering the bytes in events increases the compression ratio ~2x.

A normal slp file is a stream of events consisting of a command byte and statically sized payload.
Event payloads are almost all the same, so we can reorder the data to increase the compressability of the data.

We first turn the event stream into a list of command bytes, keeping the order but removing the payloads.
Then a list of all of the first bytes in the payload for event with command 0, 
then all of the second bytes in the payload for event with command 0,
all the way to a list of the last bytes of the payload for command 255.

To undo this reordering we also need the number of total events, so we put this in as 4 bytes at the start.

## Example
```
cmd ABCD cmd2 EFG cmd ABCD cmd3 HI cmd2 EFG
```

converts to:
```
// Event Order
5 cmd cmd2 cmd cmd3 cmd2
// Reordered Event Data
AABBCCDD EEFFGG HI
```
