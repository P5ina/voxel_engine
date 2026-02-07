# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build                # Debug build
cargo build --release      # Optimized build (needed for playable framerate)
cargo run --release        # Run the engine
cargo clippy               # Lint
cargo fmt                  # Format
cargo test                 # Run tests (no test suite yet)
```

Single crate, no workspace, no custom build scripts, no feature flags. Rust 2024 edition.

## Architecture Overview

Custom voxel engine ("THE DROP") — extraction shooter with destructible terrain and path-traced rendering. No ECS framework; state lives in `AppState` (src/lib.rs).

### Module Map

- **src/lib.rs** — `AppState` struct, event loop, game logic (~2500 lines, the hub of everything)
- **src/voxel/** — Chunk/Column storage (`Chunk` = 32³ section, `Column` = sparse 32-section vertical), greedy mesher, raycast
- **src/world/** — `ChunkManager` (column-based HashMap storage), `VoxelOctree` (LOD hierarchy), `ChunkStreamer`, save/load
- **src/pathtracer/** — Hybrid path tracer: G-buffer rasterization → compute shader lighting → accumulate → denoise → tonemap
- **src/renderer/** — wgpu setup (`RenderContext`), vertex formats, GPU resource types
- **src/character/** — Polygonal character models (GLTF), first-person arms
- **src/model/** — GLTF/GLB loading, skeletal animation
- **src/bvh/** — BVH for ray-triangle intersection (characters in path tracer)
- **src/ui/** — egui-based UI, editor tools, screen state machine

### Shader Pipeline (6 WGSL files in src/)

`pt_gbuffer.wgsl` → `pt_pathtrace.wgsl` or `pt_direct.wgsl` → `pt_accumulate.wgsl` → `pt_denoise.wgsl` → `pt_tonemap.wgsl`

G-buffer rasterizes voxel meshes, compute shaders do lighting (path trace or direct), temporal accumulation reduces noise, edge-aware denoise, ACES tonemap to screen.

### Core Data Model

- `Voxel = u8` — material palette index (0 = air, 1-255 = materials)
- `Chunk` (aka `Section`) — 32×32×32 voxel array (32KB), `VOXEL_SCALE = 1/2` (2 voxels per meter)
- `Column` — sparse array of up to 32 sections (32×1024×32 voxels), stored as `[Option<Box<Section>>; 32]`
- `ColumnPos` — i32 XZ column coordinates; `ChunkPosition` — i32 XYZ section coordinates
- `LodNodeKey` — chunk coords + LOD level
- `VoxelData` enum — `Full(32³)`, `Lod1(16³)`, `Lod2(8³)`, `Lod3(4³)`, `Homogeneous(u8)`
- `MeshKey` enum — `Chunk(ChunkPosition)` | `LodNode(LodNodeKey)` for unified mesh storage

### Octree Child Navigation

Nodes use `children_mask` (u8 bitfield) + `data_index`. To find child `i`: `data_index + (children_mask & ((1 << i) - 1)).count_ones()`. Copy node fields to locals before recursive `&mut self` calls (borrow checker).

### Threading Model

Background work (meshing, save/load) dispatched to rayon thread pool via `rayon::spawn`. Results sent back via `mpsc` channels. Main thread does GPU uploads only. Neighbor snapshots use 9-column (3×3 XZ) lookups with Y-neighbors via array index within column. `streaming_inflight: HashSet<ChunkPosition>` prevents duplicate section dispatch. All chunks come from `RegionManager` loading region files from disk — no runtime procedural generation.

### Save/Load Format

Region-based: `maps/worldname/world.meta` + `maps/worldname/regions/r_X_Z.region`. World meta uses `REGWORLD` magic + LZ4(bincode). Region files use `VXREGION` magic + LZ4(bincode(Vec<CompressedColumn>)) with per-section RLE. Legacy `BIGWFAST` format still supported for loading. `VoxelData` has custom serde (tag byte + raw bytes).

### World Dimensions

512×32×512 chunks = 1024×512×1024 meters. `WORLD_HEIGHT_CHUNKS = 32`. LOD distances: [32, 64, 160, 384, 768, 1024] meters across 6 levels. Streaming is XZ-only (all Y sections emitted for each desired column). Maps saved to `maps/` directory.
