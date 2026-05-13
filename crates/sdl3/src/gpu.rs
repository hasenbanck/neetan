use std::{ffi::CStr, marker::PhantomData, os::raw::c_void};

pub use sdl3_sys::gpu::{
    // Enum values.
    SDL_GPU_CULLMODE_NONE,
    SDL_GPU_FILLMODE_FILL,
    SDL_GPU_FILTER_LINEAR,
    SDL_GPU_FILTER_NEAREST,
    SDL_GPU_FRONTFACE_COUNTER_CLOCKWISE,
    SDL_GPU_LOADOP_CLEAR,
    SDL_GPU_PRESENTMODE_IMMEDIATE,
    SDL_GPU_PRESENTMODE_VSYNC,
    SDL_GPU_PRIMITIVETYPE_TRIANGLELIST,
    SDL_GPU_SAMPLECOUNT_1,
    SDL_GPU_SAMPLERADDRESSMODE_CLAMP_TO_EDGE,
    SDL_GPU_SAMPLERMIPMAPMODE_NEAREST,
    SDL_GPU_SHADERFORMAT_DXIL,
    SDL_GPU_SHADERFORMAT_METALLIB,
    SDL_GPU_SHADERFORMAT_MSL,
    SDL_GPU_SHADERFORMAT_SPIRV,
    SDL_GPU_SHADERSTAGE_FRAGMENT,
    SDL_GPU_SHADERSTAGE_VERTEX,
    SDL_GPU_STOREOP_STORE,
    SDL_GPU_SWAPCHAINCOMPOSITION_HDR_EXTENDED_LINEAR,
    SDL_GPU_SWAPCHAINCOMPOSITION_HDR10_ST2084,
    SDL_GPU_SWAPCHAINCOMPOSITION_SDR,
    SDL_GPU_SWAPCHAINCOMPOSITION_SDR_LINEAR,
    SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM_SRGB,
    SDL_GPU_TEXTURETYPE_2D,
    SDL_GPU_TEXTUREUSAGE_COLOR_TARGET,
    SDL_GPU_TEXTUREUSAGE_SAMPLER,
    SDL_GPU_TRANSFERBUFFERUSAGE_UPLOAD,
    SDL_GPUColorComponentFlags,
    SDL_GPUColorTargetBlendState,
    SDL_GPUColorTargetDescription,
    SDL_GPUCullMode,
    SDL_GPUDepthStencilState,
    SDL_GPUFillMode,
    SDL_GPUFilter,
    SDL_GPUFrontFace,
    SDL_GPUGraphicsPipelineTargetInfo,
    SDL_GPULoadOp,
    SDL_GPUMultisampleState,
    SDL_GPUPresentMode,
    SDL_GPUPrimitiveType,
    SDL_GPURasterizerState,
    SDL_GPUSampleCount,
    SDL_GPUSamplerAddressMode,
    SDL_GPUSamplerCreateInfo,
    SDL_GPUSamplerMipmapMode,
    SDL_GPUShaderFormat,
    SDL_GPUShaderStage,
    SDL_GPUStoreOp,
    SDL_GPUSwapchainComposition,
    SDL_GPUTextureCreateInfo,
    SDL_GPUTextureFormat,
    SDL_GPUTextureType,
    SDL_GPUTextureUsageFlags,
    SDL_GPUTransferBufferCreateInfo,
    SDL_GPUTransferBufferUsage,
    SDL_GPUVertexInputState,
    // Property names.
    SDL_PROP_GPU_DEVICE_CREATE_D3D12_ALLOW_FEWER_RESOURCE_SLOTS_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_DEBUGMODE_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_FEATURE_ANISOTROPY_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_FEATURE_CLIP_DISTANCE_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_FEATURE_DEPTH_CLAMPING_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_FEATURE_INDIRECT_DRAW_FIRST_INSTANCE_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_SHADERS_DXIL_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_SHADERS_METALLIB_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_SHADERS_MSL_BOOLEAN,
    SDL_PROP_GPU_DEVICE_CREATE_SHADERS_SPIRV_BOOLEAN,
};
use sdl3_sys::{gpu as ffi, video::SDL_Window};
pub use sdl3_sys::{pixels::SDL_FColor, video::SDL_Window as RawWindow};

use crate::{Error, properties::Properties, video::Window};

/// An SDL3 GPU device. Calls `SDL_DestroyGPUDevice` on drop.
pub struct GpuDevice {
    ptr: *mut ffi::SDL_GPUDevice,
    _marker: PhantomData<*mut ()>,
}

impl GpuDevice {
    /// Creates a GPU device from the given [`Properties`] set.
    pub fn with_properties(properties: &Properties) -> Result<Self, Error> {
        // Safety: properties id is valid for the lifetime of the borrow.
        let ptr = unsafe { ffi::SDL_CreateGPUDeviceWithProperties(properties.as_raw()) };
        if ptr.is_null() {
            return Err(crate::get_error());
        }
        Ok(Self {
            ptr,
            _marker: PhantomData,
        })
    }

    /// Attaches the given window to this device for swapchain presentation.
    pub fn claim_window(&self, window: &Window) -> Result<(), Error> {
        // Safety: device and window pointers are valid for the borrows.
        let ok = unsafe { ffi::SDL_ClaimWindowForGPUDevice(self.ptr, window.raw()) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Releases a window from this device by raw pointer. This is the only
    /// sanctioned `unsafe` in the GPU API; it exists so a `Drop` impl that
    /// outlives its `Window` value can still detach itself from the device.
    /// Idempotent; never fails.
    ///
    /// # Safety
    ///
    /// `window` must be a `SDL_Window` pointer previously claimed by this
    /// device, or null. The pointer does not need to refer to a live window
    /// (SDL tolerates a stale handle here).
    pub unsafe fn release_window_raw(&self, window: *mut SDL_Window) {
        // Safety: caller upholds the invariant above.
        unsafe { ffi::SDL_ReleaseWindowFromGPUDevice(self.ptr, window) }
    }

    /// Configures swapchain present mode and color composition.
    pub fn set_swapchain_parameters(
        &self,
        window: &Window,
        composition: SDL_GPUSwapchainComposition,
        present_mode: SDL_GPUPresentMode,
    ) -> Result<(), Error> {
        // Safety: device and window pointers are valid for the borrows.
        let ok = unsafe {
            ffi::SDL_SetGPUSwapchainParameters(self.ptr, window.raw(), composition, present_mode)
        };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Returns true if the given window supports the requested swapchain composition.
    pub fn window_supports_swapchain_composition(
        &self,
        window: &Window,
        composition: SDL_GPUSwapchainComposition,
    ) -> bool {
        // Safety: device and window pointers are valid for the borrows.
        unsafe {
            ffi::SDL_WindowSupportsGPUSwapchainComposition(self.ptr, window.raw(), composition)
        }
    }

    /// Returns true if the given window supports the requested present mode.
    pub fn window_supports_present_mode(
        &self,
        window: &Window,
        present_mode: SDL_GPUPresentMode,
    ) -> bool {
        // Safety: device and window pointers are valid for the borrows.
        unsafe { ffi::SDL_WindowSupportsGPUPresentMode(self.ptr, window.raw(), present_mode) }
    }

    /// Sets the maximum number of frames in flight. SDL accepts the range 1..=3;
    /// out-of-range values are clamped before being passed to SDL. Calling this
    /// stalls and flushes the command queue.
    pub fn set_allowed_frames_in_flight(&self, count: u32) -> Result<(), Error> {
        let clamped = count.clamp(1, 3);
        // Safety: device pointer is valid; SDL accepts any value in 1..=3.
        let ok = unsafe { ffi::SDL_SetGPUAllowedFramesInFlight(self.ptr, clamped) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }

    /// Returns the swapchain texture format for the given claimed window.
    pub fn swapchain_texture_format(&self, window: &Window) -> SDL_GPUTextureFormat {
        // Safety: device and window pointers are valid for the borrows.
        unsafe { ffi::SDL_GetGPUSwapchainTextureFormat(self.ptr, window.raw()) }
    }

    /// Returns the bitmask of shader formats this device accepts.
    pub fn shader_formats(&self) -> SDL_GPUShaderFormat {
        // Safety: device pointer is valid.
        unsafe { ffi::SDL_GetGPUShaderFormats(self.ptr) }
    }

    /// Creates a graphics shader (vertex or fragment) from precompiled bytecode.
    pub fn create_shader(&self, descriptor: &ShaderDescriptor<'_>) -> Result<GpuShader, Error> {
        let info = ffi::SDL_GPUShaderCreateInfo {
            code_size: descriptor.code.len(),
            code: descriptor.code.as_ptr(),
            entrypoint: descriptor.entrypoint.as_ptr(),
            format: descriptor.format,
            stage: descriptor.stage,
            num_samplers: descriptor.num_samplers,
            num_storage_textures: descriptor.num_storage_textures,
            num_storage_buffers: descriptor.num_storage_buffers,
            num_uniform_buffers: descriptor.num_uniform_buffers,
            ..Default::default()
        };
        // Safety: code slice and entrypoint CStr live for the duration of the call;
        // code_size matches the slice length and the bytecode format byte interpretation
        // is the device's responsibility (it returns null on a format/size mismatch).
        let ptr = unsafe { ffi::SDL_CreateGPUShader(self.ptr, &raw const info) };
        if ptr.is_null() {
            return Err(crate::get_error());
        }
        Ok(GpuShader {
            ptr,
            device: self.ptr,
            _marker: PhantomData,
        })
    }

    /// Creates a graphics pipeline from the given descriptor.
    pub fn create_graphics_pipeline(
        &self,
        descriptor: &GraphicsPipelineDescriptor<'_>,
    ) -> Result<GpuGraphicsPipeline, Error> {
        let info = ffi::SDL_GPUGraphicsPipelineCreateInfo {
            vertex_shader: descriptor.vertex_shader.raw_handle(),
            fragment_shader: descriptor.fragment_shader.raw_handle(),
            vertex_input_state: descriptor.vertex_input_state,
            primitive_type: descriptor.primitive_type,
            rasterizer_state: descriptor.rasterizer_state,
            multisample_state: descriptor.multisample_state,
            depth_stencil_state: descriptor.depth_stencil_state,
            target_info: descriptor.target_info,
            ..Default::default()
        };
        // Safety: device pointer is valid; info is a valid borrow whose internal
        // pointers were obtained from live wrappers via raw_handle().
        let ptr = unsafe { ffi::SDL_CreateGPUGraphicsPipeline(self.ptr, &raw const info) };
        if ptr.is_null() {
            return Err(crate::get_error());
        }
        Ok(GpuGraphicsPipeline {
            ptr,
            device: self.ptr,
            _marker: PhantomData,
        })
    }

    /// Creates a sampler from the given description.
    pub fn create_sampler(&self, info: &SDL_GPUSamplerCreateInfo) -> Result<GpuSampler, Error> {
        // Safety: device pointer is valid; info is a valid borrow.
        let ptr = unsafe { ffi::SDL_CreateGPUSampler(self.ptr, info as *const _) };
        if ptr.is_null() {
            return Err(crate::get_error());
        }
        Ok(GpuSampler {
            ptr,
            device: self.ptr,
            _marker: PhantomData,
        })
    }

    /// Creates a GPU texture from the given description.
    pub fn create_texture(&self, info: &SDL_GPUTextureCreateInfo) -> Result<GpuTexture, Error> {
        // Safety: device pointer is valid; info is a valid borrow.
        let ptr = unsafe { ffi::SDL_CreateGPUTexture(self.ptr, info as *const _) };
        if ptr.is_null() {
            return Err(crate::get_error());
        }
        Ok(GpuTexture {
            ptr,
            device: self.ptr,
            _marker: PhantomData,
        })
    }

    /// Creates a transfer buffer (staging buffer) from the given description.
    pub fn create_transfer_buffer(
        &self,
        info: &SDL_GPUTransferBufferCreateInfo,
    ) -> Result<GpuTransferBuffer, Error> {
        // Safety: device pointer is valid; info is a valid borrow.
        let ptr = unsafe { ffi::SDL_CreateGPUTransferBuffer(self.ptr, info as *const _) };
        if ptr.is_null() {
            return Err(crate::get_error());
        }
        Ok(GpuTransferBuffer {
            ptr,
            device: self.ptr,
            size: info.size,
            _marker: PhantomData,
        })
    }

    /// Maps a transfer buffer for CPU writes. `cycle=true` discards prior
    /// contents and avoids GPU/CPU sync stalls when overwriting every frame.
    pub fn map_transfer_buffer(
        &self,
        buffer: &GpuTransferBuffer,
        cycle: bool,
    ) -> Result<*mut u8, Error> {
        // Safety: device and buffer pointers are valid.
        let ptr = unsafe { ffi::SDL_MapGPUTransferBuffer(self.ptr, buffer.ptr, cycle) };
        if ptr.is_null() {
            return Err(crate::get_error());
        }
        Ok(ptr.cast::<u8>())
    }

    /// Unmaps a previously mapped transfer buffer.
    pub fn unmap_transfer_buffer(&self, buffer: &GpuTransferBuffer) {
        // Safety: device and buffer pointers are valid.
        unsafe { ffi::SDL_UnmapGPUTransferBuffer(self.ptr, buffer.ptr) }
    }

    /// Acquires a command buffer from the device's command pool.
    pub fn acquire_command_buffer(&self) -> Result<GpuCommandBuffer, Error> {
        // Safety: device pointer is valid.
        let ptr = unsafe { ffi::SDL_AcquireGPUCommandBuffer(self.ptr) };
        if ptr.is_null() {
            return Err(crate::get_error());
        }
        Ok(GpuCommandBuffer {
            ptr,
            _marker: PhantomData,
        })
    }
}

impl Drop for GpuDevice {
    fn drop(&mut self) {
        // Safety: device pointer is valid and we own it.
        unsafe { ffi::SDL_DestroyGPUDevice(self.ptr) }
    }
}

/// A GPU shader (vertex or fragment). Released back to the device on drop.
pub struct GpuShader {
    ptr: *mut ffi::SDL_GPUShader,
    device: *mut ffi::SDL_GPUDevice,
    _marker: PhantomData<*mut ()>,
}

impl GpuShader {
    fn raw_handle(&self) -> *mut ffi::SDL_GPUShader {
        self.ptr
    }
}

impl Drop for GpuShader {
    fn drop(&mut self) {
        // Safety: device and shader pointers are valid.
        unsafe { ffi::SDL_ReleaseGPUShader(self.device, self.ptr) }
    }
}

/// A graphics pipeline. Released back to the device on drop.
pub struct GpuGraphicsPipeline {
    ptr: *mut ffi::SDL_GPUGraphicsPipeline,
    device: *mut ffi::SDL_GPUDevice,
    _marker: PhantomData<*mut ()>,
}

impl GpuGraphicsPipeline {
    fn raw_handle(&self) -> *mut ffi::SDL_GPUGraphicsPipeline {
        self.ptr
    }
}

impl Drop for GpuGraphicsPipeline {
    fn drop(&mut self) {
        // Safety: device and pipeline pointers are valid.
        unsafe { ffi::SDL_ReleaseGPUGraphicsPipeline(self.device, self.ptr) }
    }
}

/// A sampler. Released back to the device on drop.
pub struct GpuSampler {
    ptr: *mut ffi::SDL_GPUSampler,
    device: *mut ffi::SDL_GPUDevice,
    _marker: PhantomData<*mut ()>,
}

impl GpuSampler {
    fn raw_handle(&self) -> *mut ffi::SDL_GPUSampler {
        self.ptr
    }
}

impl Drop for GpuSampler {
    fn drop(&mut self) {
        // Safety: device and sampler pointers are valid.
        unsafe { ffi::SDL_ReleaseGPUSampler(self.device, self.ptr) }
    }
}

/// A GPU texture. Released back to the device on drop.
pub struct GpuTexture {
    ptr: *mut ffi::SDL_GPUTexture,
    device: *mut ffi::SDL_GPUDevice,
    _marker: PhantomData<*mut ()>,
}

impl GpuTexture {
    fn raw_handle(&self) -> *mut ffi::SDL_GPUTexture {
        self.ptr
    }
}

impl Drop for GpuTexture {
    fn drop(&mut self) {
        // Safety: device and texture pointers are valid.
        unsafe { ffi::SDL_ReleaseGPUTexture(self.device, self.ptr) }
    }
}

/// A CPU-mappable transfer (staging) buffer. Released back to the device on drop.
pub struct GpuTransferBuffer {
    ptr: *mut ffi::SDL_GPUTransferBuffer,
    device: *mut ffi::SDL_GPUDevice,
    size: u32,
    _marker: PhantomData<*mut ()>,
}

impl GpuTransferBuffer {
    fn raw_handle(&self) -> *mut ffi::SDL_GPUTransferBuffer {
        self.ptr
    }

    /// Returns the requested size in bytes.
    pub fn size(&self) -> u32 {
        self.size
    }
}

impl Drop for GpuTransferBuffer {
    fn drop(&mut self) {
        // Safety: device and buffer pointers are valid.
        unsafe { ffi::SDL_ReleaseGPUTransferBuffer(self.device, self.ptr) }
    }
}

/// A command buffer for recording GPU work. Consumed by [`submit`].
///
/// SDL3 owns the command buffer lifetime; dropping without submitting leaks
/// it (an SDL warning may be issued). Always call `submit` to finalize.
///
/// [`submit`]: GpuCommandBuffer::submit
pub struct GpuCommandBuffer {
    ptr: *mut ffi::SDL_GPUCommandBuffer,
    _marker: PhantomData<*mut ()>,
}

impl GpuCommandBuffer {
    /// Waits for and acquires the next swapchain texture for the given window.
    ///
    /// On success returns `Some(SwapchainTexture)` if a texture is available,
    /// or `None` if the window is currently unable to present (e.g. minimized).
    /// The caller must still submit the command buffer in the `None` case.
    pub fn wait_and_acquire_swapchain_texture(
        &mut self,
        window: &Window,
    ) -> Result<Option<SwapchainTexture>, Error> {
        let mut texture: *mut ffi::SDL_GPUTexture = std::ptr::null_mut();
        let mut width: u32 = 0;
        let mut height: u32 = 0;
        // Safety: cb and window pointers are valid for the borrows; out-params are valid.
        let ok = unsafe {
            ffi::SDL_WaitAndAcquireGPUSwapchainTexture(
                self.ptr,
                window.raw(),
                &raw mut texture,
                &raw mut width,
                &raw mut height,
            )
        };
        if !ok {
            return Err(crate::get_error());
        }
        if texture.is_null() {
            return Ok(None);
        }
        Ok(Some(SwapchainTexture {
            handle: texture,
            width,
            height,
        }))
    }

    /// Begins a copy pass. Must be ended (by dropping the returned guard)
    /// before any render pass or `submit` call.
    pub fn begin_copy_pass(&mut self) -> GpuCopyPass<'_> {
        // Safety: cb pointer is valid; SDL3 always returns a valid copy pass.
        let ptr = unsafe { ffi::SDL_BeginGPUCopyPass(self.ptr) };
        GpuCopyPass {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Begins a render pass with a single color target.
    pub fn begin_render_pass(
        &mut self,
        color_target: &ColorTargetDescriptor<'_>,
    ) -> GpuRenderPass<'_> {
        let info = ffi::SDL_GPUColorTargetInfo {
            texture: color_target.texture.raw_handle(),
            clear_color: color_target.clear_color,
            load_op: color_target.load_op,
            store_op: color_target.store_op,
            ..Default::default()
        };
        // Safety: cb pointer is valid; info is built from live wrappers; no depth/stencil.
        let ptr =
            unsafe { ffi::SDL_BeginGPURenderPass(self.ptr, &raw const info, 1, std::ptr::null()) };
        GpuRenderPass {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Pushes fragment uniform data for the given slot. Replaces push constants.
    pub fn push_fragment_uniform_data(&self, slot_index: u32, data: &[u8]) {
        // Safety: cb pointer is valid; data slice covers `data.len()` bytes.
        unsafe {
            ffi::SDL_PushGPUFragmentUniformData(
                self.ptr,
                slot_index,
                data.as_ptr() as *const c_void,
                data.len() as u32,
            )
        }
    }

    /// Submits this command buffer to the GPU. Consumes self.
    pub fn submit(self) -> Result<(), Error> {
        // Safety: cb pointer is valid; SDL takes ownership on submit.
        let ok = unsafe { ffi::SDL_SubmitGPUCommandBuffer(self.ptr) };
        if !ok {
            return Err(crate::get_error());
        }
        Ok(())
    }
}

/// Active copy pass scope. Ends on drop.
pub struct GpuCopyPass<'a> {
    ptr: *mut ffi::SDL_GPUCopyPass,
    _marker: PhantomData<&'a mut GpuCommandBuffer>,
}

impl GpuCopyPass<'_> {
    /// Uploads data from a transfer buffer region into a texture region.
    pub fn upload_to_texture(
        &self,
        source: &TextureTransferInfo<'_>,
        destination: &TextureRegion<'_>,
        cycle: bool,
    ) {
        let source_info = ffi::SDL_GPUTextureTransferInfo {
            transfer_buffer: source.transfer_buffer.raw_handle(),
            offset: source.offset,
            pixels_per_row: source.pixels_per_row,
            rows_per_layer: source.rows_per_layer,
        };
        let destination_region = ffi::SDL_GPUTextureRegion {
            texture: destination.texture.raw_handle(),
            mip_level: destination.mip_level,
            layer: destination.layer,
            x: destination.x,
            y: destination.y,
            z: destination.z,
            w: destination.w,
            h: destination.h,
            d: destination.d,
        };
        // Safety: copy pass pointer is valid; the FFI structs were built from
        // live wrapper handles.
        unsafe {
            ffi::SDL_UploadToGPUTexture(
                self.ptr,
                &raw const source_info,
                &raw const destination_region,
                cycle,
            )
        }
    }
}

impl Drop for GpuCopyPass<'_> {
    fn drop(&mut self) {
        // Safety: copy pass pointer is valid; ending an in-flight pass is the documented contract.
        unsafe { ffi::SDL_EndGPUCopyPass(self.ptr) }
    }
}

/// Active render pass scope. Ends on drop.
pub struct GpuRenderPass<'a> {
    ptr: *mut ffi::SDL_GPURenderPass,
    _marker: PhantomData<&'a mut GpuCommandBuffer>,
}

impl GpuRenderPass<'_> {
    /// Binds a graphics pipeline for subsequent draws in this pass.
    pub fn bind_graphics_pipeline(&self, pipeline: &GpuGraphicsPipeline) {
        // Safety: render pass and pipeline pointers are valid.
        unsafe { ffi::SDL_BindGPUGraphicsPipeline(self.ptr, pipeline.raw_handle()) }
    }

    /// Binds fragment-stage (texture, sampler) pairs starting at `first_slot`.
    pub fn bind_fragment_samplers(&self, first_slot: u32, bindings: &[TextureSamplerBinding<'_>]) {
        // Build a stack-friendly FFI slice. The present pass uses 2 bindings;
        // anything larger spills to a heap Vec without disturbing the call site.
        const STACK_CAPACITY: usize = 8;
        let mut stack_buffer: [ffi::SDL_GPUTextureSamplerBinding; STACK_CAPACITY] =
            [ffi::SDL_GPUTextureSamplerBinding {
                texture: std::ptr::null_mut(),
                sampler: std::ptr::null_mut(),
            }; STACK_CAPACITY];
        let ffi_slice: &[ffi::SDL_GPUTextureSamplerBinding] = if bindings.len() <= STACK_CAPACITY {
            for (slot, binding) in bindings.iter().enumerate() {
                stack_buffer[slot] = ffi::SDL_GPUTextureSamplerBinding {
                    texture: binding.texture.raw_handle(),
                    sampler: binding.sampler.raw_handle(),
                };
            }
            &stack_buffer[..bindings.len()]
        } else {
            let heap: Vec<ffi::SDL_GPUTextureSamplerBinding> = bindings
                .iter()
                .map(|binding| ffi::SDL_GPUTextureSamplerBinding {
                    texture: binding.texture.raw_handle(),
                    sampler: binding.sampler.raw_handle(),
                })
                .collect();
            // Safety: render pass pointer is valid; the heap slice outlives the FFI call.
            unsafe {
                ffi::SDL_BindGPUFragmentSamplers(
                    self.ptr,
                    first_slot,
                    heap.as_ptr(),
                    heap.len() as u32,
                )
            }
            return;
        };
        // Safety: render pass pointer is valid; the stack buffer outlives the FFI call.
        unsafe {
            ffi::SDL_BindGPUFragmentSamplers(
                self.ptr,
                first_slot,
                ffi_slice.as_ptr(),
                ffi_slice.len() as u32,
            )
        }
    }

    /// Issues a non-indexed draw call.
    pub fn draw_primitives(
        &self,
        num_vertices: u32,
        num_instances: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        // Safety: render pass pointer is valid.
        unsafe {
            ffi::SDL_DrawGPUPrimitives(
                self.ptr,
                num_vertices,
                num_instances,
                first_vertex,
                first_instance,
            )
        }
    }
}

impl Drop for GpuRenderPass<'_> {
    fn drop(&mut self) {
        // Safety: render pass pointer is valid.
        unsafe { ffi::SDL_EndGPURenderPass(self.ptr) }
    }
}

/// Description of a shader to create.
pub struct ShaderDescriptor<'a> {
    /// Precompiled bytecode in `format`.
    pub code: &'a [u8],
    /// Entry-point symbol in the bytecode.
    pub entrypoint: &'a CStr,
    /// Bytecode format (SPIR-V, DXIL, metallib, ...).
    pub format: SDL_GPUShaderFormat,
    /// Pipeline stage this shader is for.
    pub stage: SDL_GPUShaderStage,
    /// Number of (texture, sampler) bindings the shader reads.
    pub num_samplers: u32,
    /// Number of storage textures the shader reads.
    pub num_storage_textures: u32,
    /// Number of storage buffers the shader reads.
    pub num_storage_buffers: u32,
    /// Number of uniform-buffer slots the shader reads.
    pub num_uniform_buffers: u32,
}

/// Borrowed handle for the current swapchain image. Owned by SDL; value
/// validity is informal (it lasts until the command buffer is submitted).
pub struct SwapchainTexture {
    handle: *mut ffi::SDL_GPUTexture,
    width: u32,
    height: u32,
}

impl SwapchainTexture {
    /// Returns the swapchain texture width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the swapchain texture height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    fn raw_handle(&self) -> *mut ffi::SDL_GPUTexture {
        self.handle
    }
}

/// Description of a render pass's single color target.
pub struct ColorTargetDescriptor<'a> {
    /// Swapchain texture to render into.
    pub texture: &'a SwapchainTexture,
    /// Clear color used when `load_op` is `SDL_GPU_LOADOP_CLEAR`.
    pub clear_color: SDL_FColor,
    /// Load operation at the start of the pass.
    pub load_op: SDL_GPULoadOp,
    /// Store operation at the end of the pass.
    pub store_op: SDL_GPUStoreOp,
}

/// Description of a graphics pipeline to create.
pub struct GraphicsPipelineDescriptor<'a> {
    /// Vertex shader.
    pub vertex_shader: &'a GpuShader,
    /// Fragment shader.
    pub fragment_shader: &'a GpuShader,
    /// Vertex input layout.
    pub vertex_input_state: SDL_GPUVertexInputState,
    /// Primitive topology.
    pub primitive_type: SDL_GPUPrimitiveType,
    /// Rasterizer state.
    pub rasterizer_state: SDL_GPURasterizerState,
    /// Multisample state.
    pub multisample_state: SDL_GPUMultisampleState,
    /// Depth/stencil state.
    pub depth_stencil_state: SDL_GPUDepthStencilState,
    /// Render-target description (color formats, blend, depth).
    pub target_info: SDL_GPUGraphicsPipelineTargetInfo,
}

/// A (texture, sampler) pair bound at a single fragment-stage slot.
pub struct TextureSamplerBinding<'a> {
    /// Texture to sample.
    pub texture: &'a GpuTexture,
    /// Sampler describing filtering and addressing.
    pub sampler: &'a GpuSampler,
}

/// Source range of a `upload_to_texture` copy.
pub struct TextureTransferInfo<'a> {
    /// Staging buffer containing the source bytes.
    pub transfer_buffer: &'a GpuTransferBuffer,
    /// Byte offset into the transfer buffer.
    pub offset: u32,
    /// Source image width in pixels.
    pub pixels_per_row: u32,
    /// Source image height in pixels.
    pub rows_per_layer: u32,
}

/// Destination region of a `upload_to_texture` copy.
pub struct TextureRegion<'a> {
    /// Destination texture.
    pub texture: &'a GpuTexture,
    /// Destination mip level.
    pub mip_level: u32,
    /// Destination array layer (or 0 for 2D textures).
    pub layer: u32,
    /// Destination x offset in pixels.
    pub x: u32,
    /// Destination y offset in pixels.
    pub y: u32,
    /// Destination z offset for 3D textures.
    pub z: u32,
    /// Width to copy in pixels.
    pub w: u32,
    /// Height to copy in pixels.
    pub h: u32,
    /// Depth to copy for 3D textures.
    pub d: u32,
}
