# ADR-012: Device lifetime for multiple native windows

Status: accepted, 2026-10-17. Windows validation and remaining limitations are described below.

## Context

Repeated creation and destruction of native GPU windows currently creates a new
logical graphics device for each window. Resource observations on Windows show
increasing handle counts during repeated device creation, including without the
document host. Reusing an Instance or Adapter alone does not remove that behavior.
These observations do not identify the allocator and are not a guarantee that a
shared device eliminates every resource-lifetime issue.

## Decision

`GpuContext` is an explicit host-owned cache of one Instance, Adapter, Device and
Queue generation. It owns no Window or Surface. `GpuRenderer::new_with_context`
creates a compatible surface and local draw resources for each native window.
`GpuRenderer::new` remains available with an independent context for existing
single-window callers.

The multi-window host retains one context while windows are added and retired.
Every renderer retains its own Surface, pipelines, atlas, image cache and buffers.
Retirement drops those per-window objects before the native window. The context
does not keep a retired window alive and is not a process-global singleton.

Surface compatibility is checked against the shared adapter. Unsupported surfaces
and validation errors remain errors; they do not silently select software drawing
or create an unrelated per-window device.

## Recovery

A device loss marks the shared health state. Controlled destruction marks it
synchronously before calling Device destruction, because a backend callback may
arrive later. Rebuilding a lost context requests a fresh Adapter and Device and
advances a checked generation. The host recreates every renderer using the old
generation, including its device-bound caches, then requests redraws. Window and
application content identities stay unchanged. Surface loss alone recreates that
surface using the healthy context.

The host keeps its bounded recovery policy. Validation errors are not treated as
loss and must remain visible to the caller. A callback only records health state;
it does not call application code or borrow the host's context.

## Validation boundary

Required checks include multiple simultaneous surfaces, repeated retirement,
both Windows backends, controlled device loss while other windows are idle,
surface recreation, resize and retained application state. Unit generation checks
do not substitute these native checks. No device sharing across independent host
instances or cross-adapter migration is introduced by this decision.

Controlled single/multiple-window recovery has been checked on Windows DX12 and
Vulkan, including actual Present and one image upload per device generation in
each window. Repeated window retirement still shows residual Windows/Vulkan
process-handle growth. This decision improves device lifetime but does not close
that [long-running host limitation](../document-workspace.md#long-running-native-window-limitation).
