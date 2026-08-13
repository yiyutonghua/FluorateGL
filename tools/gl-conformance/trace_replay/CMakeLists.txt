cmake_minimum_required(VERSION 3.22.1)

if(NOT UNIX OR ANDROID)
    message(STATUS "FluorateGL trace replay runner is only enabled on desktop Unix")
    return()
endif()

set(FLUORATEGL_TRACE_ROOT "${CMAKE_CURRENT_LIST_DIR}")
set(FLUORATEGL_TRACE_SHARED_CPP_DIR "${CMAKE_CURRENT_LIST_DIR}")
set(APITRACE_ROOT "${CMAKE_SOURCE_DIR}/3rdparty/apitrace" CACHE PATH "apitrace source tree (KhronosGroup/apitrace or MobileGL fork)")
set(APITRACE_BINARY_DIR "${CMAKE_CURRENT_BINARY_DIR}/apitrace")
set(APITRACE_VERSION "fluorategl-trace")
set(FLUORATEGL_TRACE_REPLAY_LIBRARY "" CACHE FILEPATH "Existing libfluorategl.so to use for trace replay tests")

find_package(Python3 REQUIRED)
find_package(Threads REQUIRED)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

include("${APITRACE_ROOT}/cmake/ConvenienceLibrary.cmake")

function(target_optimize TARGET)
    target_compile_definitions(${TARGET} PRIVATE -DNDEBUG)
    target_compile_options(${TARGET} PRIVATE -O3)
endfunction()

function(include_with_scope)
    include(${ARGV})
endfunction()

set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(ENABLE_STATIC_SNAPPY ON CACHE BOOL "" FORCE)
set(DOC_INSTALL_DIR "doc" CACHE PATH "" FORCE)
set(HAVE_X86 OFF CACHE BOOL "" FORCE)
set(ZLIB_FOUND OFF CACHE BOOL "" FORCE)
set(PNG_FOUND OFF CACHE BOOL "" FORCE)
set(Snappy_FOUND OFF CACHE BOOL "" FORCE)
set(BROTLIDEC_FOUND OFF CACHE BOOL "" FORCE)
set(BROTLIENC_FOUND OFF CACHE BOOL "" FORCE)
set(ZSTD_FOUND OFF CACHE BOOL "" FORCE)

if(NOT TARGET check)
    add_custom_target(check)
endif()
add_subdirectory("${APITRACE_ROOT}/thirdparty" "${APITRACE_BINARY_DIR}/thirdparty")

set(APITRACE_GENERATED_DIR "${APITRACE_BINARY_DIR}/generated")
file(MAKE_DIRECTORY "${APITRACE_GENERATED_DIR}")
configure_file("${APITRACE_ROOT}/version.h.in" "${APITRACE_GENERATED_DIR}/version.h" @ONLY)

add_custom_command(
        OUTPUT
        "${APITRACE_GENERATED_DIR}/glproc.hpp"
        "${APITRACE_GENERATED_DIR}/glproc.cpp"
        COMMAND ${Python3_EXECUTABLE}
        "${APITRACE_ROOT}/dispatch/glproc.py"
        "${APITRACE_GENERATED_DIR}/glproc.hpp"
        "${APITRACE_GENERATED_DIR}/glproc.cpp"
        DEPENDS
        "${APITRACE_ROOT}/dispatch/glproc.py"
        "${APITRACE_ROOT}/dispatch/dispatch.py"
        "${APITRACE_ROOT}/specs/wglapi.py"
        "${APITRACE_ROOT}/specs/glxapi.py"
        "${APITRACE_ROOT}/specs/cglapi.py"
        "${APITRACE_ROOT}/specs/eglapi.py"
        "${APITRACE_ROOT}/specs/glapi.py"
        "${APITRACE_ROOT}/specs/gltypes.py"
        "${APITRACE_ROOT}/specs/stdapi.py")

add_library(fluorategl_trace_apitrace_os STATIC
        "${APITRACE_ROOT}/lib/os/os_backtrace.cpp"
        "${APITRACE_ROOT}/lib/os/os_crtdbg.cpp"
        "${APITRACE_ROOT}/lib/os/os_posix.cpp")
target_include_directories(fluorategl_trace_apitrace_os PUBLIC
        "${APITRACE_ROOT}/compat"
        "${APITRACE_ROOT}/thirdparty"
        "${APITRACE_ROOT}/lib/os"
        "${APITRACE_ROOT}/lib/trace")
target_link_libraries(fluorategl_trace_apitrace_os PUBLIC Threads::Threads)

add_library(fluorategl_trace_highlight STATIC "${APITRACE_ROOT}/lib/highlight/highlight.cpp")
target_include_directories(fluorategl_trace_highlight PUBLIC "${APITRACE_ROOT}/lib/highlight")

add_library(fluorategl_trace_guids STATIC "${APITRACE_ROOT}/lib/guids/guids.cpp")
target_include_directories(fluorategl_trace_guids PUBLIC
        "${APITRACE_ROOT}/lib/guids"
        "${APITRACE_ROOT}/lib/os")

add_library(fluorategl_trace_image STATIC
        "${APITRACE_ROOT}/lib/image/image_bmp.cpp"
        "${APITRACE_ROOT}/lib/image/image_png.cpp"
        "${APITRACE_ROOT}/lib/image/image_pnm.cpp"
        "${APITRACE_ROOT}/lib/image/image_raw.cpp"
        "${APITRACE_ROOT}/lib/image/image_md5.cpp")
target_include_directories(fluorategl_trace_image PUBLIC "${APITRACE_ROOT}/lib/image")
target_link_libraries(fluorategl_trace_image PUBLIC md5 PNG::PNG)

add_library(fluorategl_trace_common STATIC
        "${APITRACE_ROOT}/lib/trace/trace_callset.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_dump.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_fast_callset.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_file.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_file_read.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_file_zlib.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_file_brotli.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_file_snappy.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_file_zstd.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_file_zstd_seekable.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_model.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_option.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_ostream_snappy.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_ostream_zlib.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_ostream_zstd.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_parser.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_parser_flags.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_parser_loop.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_profiler.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_writer.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_writer_local.cpp"
        "${APITRACE_ROOT}/lib/trace/trace_writer_model.cpp")
target_include_directories(fluorategl_trace_common PUBLIC
        "${APITRACE_ROOT}/compat"
        "${APITRACE_ROOT}/thirdparty"
        "${APITRACE_ROOT}/lib/guids"
        "${APITRACE_ROOT}/lib/highlight"
        "${APITRACE_ROOT}/lib/os"
        "${APITRACE_ROOT}/lib/trace"
        "${APITRACE_ROOT}/lib/ubjson")
target_link_libraries(fluorategl_trace_common PUBLIC
        fluorategl_trace_guids
        fluorategl_trace_highlight
        fluorategl_trace_apitrace_os
        Snappy::snappy
        ZLIB::ZLIB
        PkgConfig::BROTLIDEC
        PkgConfig::ZSTD
        zstd_seekable)

add_library(fluorategl_trace_glproc STATIC
        "${APITRACE_GENERATED_DIR}/glproc.cpp"
        "${FLUORATEGL_TRACE_ROOT}/apitrace_glproc_fluorategl.cpp")
target_include_directories(fluorategl_trace_glproc PUBLIC
        "${CMAKE_SOURCE_DIR}/include"
        "${APITRACE_GENERATED_DIR}"
        "${APITRACE_ROOT}/dispatch"
        "${APITRACE_ROOT}/lib/os"
        "${APITRACE_ROOT}/thirdparty/khronos")
target_link_libraries(fluorategl_trace_glproc PUBLIC fluorategl_trace_apitrace_os ${CMAKE_DL_LIBS})

execute_process(
        COMMAND ${Python3_EXECUTABLE} "${APITRACE_ROOT}/retrace/glretrace.py"
        OUTPUT_FILE "${APITRACE_GENERATED_DIR}/glretrace_gl.cpp"
        COMMAND_ERROR_IS_FATAL ANY)

execute_process(
        COMMAND ${Python3_EXECUTABLE} "${APITRACE_ROOT}/retrace/glstate_params.py"
        OUTPUT_FILE "${APITRACE_GENERATED_DIR}/glstate_params.cpp"
        COMMAND_ERROR_IS_FATAL ANY)

add_library(fluorategl_trace_glhelpers STATIC
        "${APITRACE_ROOT}/helpers/glfeatures.cpp"
        "${APITRACE_ROOT}/helpers/eglsize.cpp")
target_include_directories(fluorategl_trace_glhelpers PUBLIC
        "${APITRACE_GENERATED_DIR}"
        "${APITRACE_ROOT}/dispatch"
        "${APITRACE_ROOT}/helpers"
        "${APITRACE_ROOT}/lib/os"
        "${APITRACE_ROOT}/thirdparty/khronos")
target_link_libraries(fluorategl_trace_glhelpers PUBLIC fluorategl_trace_glproc fluorategl_trace_apitrace_os)

add_library(fluorategl_trace_retrace_common STATIC
        "${APITRACE_ROOT}/retrace/json.cpp"
        "${APITRACE_ROOT}/retrace/process_name.cpp"
        "${APITRACE_ROOT}/retrace/retrace.cpp"
        "${APITRACE_ROOT}/retrace/retrace_main.cpp"
        "${APITRACE_ROOT}/retrace/retrace_stdc.cpp"
        "${APITRACE_ROOT}/retrace/retrace_swizzle.cpp"
        "${APITRACE_ROOT}/retrace/state_writer.cpp"
        "${APITRACE_ROOT}/retrace/state_writer_json.cpp"
        "${APITRACE_ROOT}/retrace/state_writer_ubjson.cpp"
        "${APITRACE_ROOT}/retrace/ws.cpp")
target_compile_definitions(fluorategl_trace_retrace_common PUBLIC RETRACE)
target_compile_definitions(fluorategl_trace_retrace_common PRIVATE
        exit=fluorategl_apitrace_exit
        main=fluorategl_apitrace_main)
target_compile_options(fluorategl_trace_retrace_common PRIVATE "-include${FLUORATEGL_TRACE_SHARED_CPP_DIR}/apitrace_exit.hpp")
target_include_directories(fluorategl_trace_retrace_common PUBLIC
        "${APITRACE_GENERATED_DIR}"
        "${APITRACE_ROOT}/compat"
        "${APITRACE_ROOT}/dispatch"
        "${APITRACE_ROOT}/helpers"
        "${APITRACE_ROOT}/lib/highlight"
        "${APITRACE_ROOT}/lib/image"
        "${APITRACE_ROOT}/lib/os"
        "${APITRACE_ROOT}/lib/trace"
        "${APITRACE_ROOT}/lib/ubjson"
        "${APITRACE_ROOT}/retrace"
        "${APITRACE_ROOT}/thirdparty"
        "${APITRACE_ROOT}/thirdparty/khronos")
target_link_libraries(fluorategl_trace_retrace_common PUBLIC fluorategl_trace_image fluorategl_trace_common)

add_library(fluorategl_trace_glretrace_common STATIC
        "${APITRACE_GENERATED_DIR}/glretrace_gl.cpp"
        "${APITRACE_GENERATED_DIR}/glstate_params.cpp"
        "${APITRACE_ROOT}/retrace/glretrace_cgl.cpp"
        "${APITRACE_ROOT}/retrace/glretrace_egl.cpp"
        "${APITRACE_ROOT}/retrace/glretrace_glx.cpp"
        "${APITRACE_ROOT}/retrace/glretrace_main.cpp"
        "${APITRACE_ROOT}/retrace/glretrace_wgl.cpp"
        "${APITRACE_ROOT}/retrace/glretrace_wgl_font_bitmaps.cpp"
        "${APITRACE_ROOT}/retrace/glretrace_wgl_font_outlines.cpp"
        "${APITRACE_ROOT}/retrace/glretrace_ws.cpp"
        "${APITRACE_ROOT}/retrace/glstate.cpp"
        "${APITRACE_ROOT}/retrace/glstate_formats.cpp"
        "${APITRACE_ROOT}/retrace/glstate_images.cpp"
        "${APITRACE_ROOT}/retrace/glstate_shaders.cpp"
        "${APITRACE_ROOT}/retrace/glws.cpp"
        "${APITRACE_ROOT}/retrace/metric_backend_amd_perfmon.cpp"
        "${APITRACE_ROOT}/retrace/metric_backend_intel_perfquery.cpp"
        "${APITRACE_ROOT}/retrace/metric_backend_opengl.cpp"
        "${APITRACE_ROOT}/retrace/metric_helper.cpp"
        "${APITRACE_ROOT}/retrace/metric_writer.cpp"
        "${FLUORATEGL_TRACE_ROOT}/apitrace_fbo_dump.cpp"
        "${FLUORATEGL_TRACE_ROOT}/apitrace_glws_egl.cpp")
target_include_directories(fluorategl_trace_glretrace_common PUBLIC
        "${APITRACE_GENERATED_DIR}"
        "${APITRACE_ROOT}/dispatch"
        "${APITRACE_ROOT}/helpers"
        "${APITRACE_ROOT}/retrace")
target_compile_definitions(fluorategl_trace_glretrace_common PRIVATE
        exit=fluorategl_apitrace_exit
        main=fluorategl_apitrace_main)
target_compile_options(fluorategl_trace_glretrace_common PRIVATE "-include${FLUORATEGL_TRACE_SHARED_CPP_DIR}/apitrace_exit.hpp")
target_link_libraries(fluorategl_trace_glretrace_common PUBLIC
        fluorategl_trace_retrace_common
        fluorategl_trace_glhelpers
        fluorategl_trace_glproc)

add_executable(fluorategl_trace_replay
        "${FLUORATEGL_TRACE_SHARED_CPP_DIR}/trace_replay_core.cpp"
        "${FLUORATEGL_TRACE_ROOT}/trace_replay_cli.cpp")
target_compile_features(fluorategl_trace_replay PRIVATE cxx_std_17)
target_compile_definitions(fluorategl_trace_replay PRIVATE
        FLUORATEGL_APITRACE_RETRACE_MAIN=fluorategl_apitrace_main)
target_include_directories(fluorategl_trace_replay PRIVATE
        "${FLUORATEGL_TRACE_SHARED_CPP_DIR}"
        "${APITRACE_ROOT}/lib/image")
set(FLUORATEGL_TRACE_REPLAY_LINK_LIBRARIES
        fluorategl_trace_glretrace_common
        fluorategl_trace_retrace_common
        fluorategl_trace_image
        fluorategl_trace_common
        fluorategl_trace_glhelpers
        fluorategl_trace_glproc
        fluorategl_trace_apitrace_os
        ${CMAKE_DL_LIBS})
target_link_libraries(fluorategl_trace_replay PRIVATE
        "-Wl,--start-group"
        ${FLUORATEGL_TRACE_REPLAY_LINK_LIBRARIES}
        "-Wl,--end-group")

# FluorateGL is a Rust cdylib built by cargo, not a CMake target; the runner
# needs the absolute path of the built libfluorategl.so.
if(NOT FLUORATEGL_TRACE_REPLAY_LIBRARY)
    message(FATAL_ERROR "FLUORATEGL_TRACE_REPLAY_LIBRARY must point at the built libfluorategl.so")
endif()

enable_testing()
function(add_trace_replay_test CASE_NAME BACKEND)
    set(oneValueArgs
            TRACE_ARCHIVE
            TRACE_FILE
            GOLDEN
            ALTERNATE_GOLDEN
            TARGET_CALL
            WIDTH
            HEIGHT
            SSIM_THRESHOLD
            CROP_X
            CROP_Y
            CROP_WIDTH
            CROP_HEIGHT
            COHERENT_AS_FLUSH)
    cmake_parse_arguments(TRACE_CASE "" "${oneValueArgs}" "" ${ARGN})
    foreach(required TRACE_ARCHIVE GOLDEN TARGET_CALL WIDTH HEIGHT)
        if(NOT TRACE_CASE_${required})
            message(FATAL_ERROR "${required} is required for ${CASE_NAME}")
        endif()
    endforeach()
    if(NOT TRACE_CASE_TRACE_FILE)
        set(TRACE_CASE_TRACE_FILE trace.trace)
    endif()
    if(NOT TRACE_CASE_SSIM_THRESHOLD)
        set(TRACE_CASE_SSIM_THRESHOLD 0.99)
    endif()
    if(NOT TRACE_CASE_CROP_X)
        set(TRACE_CASE_CROP_X 0)
    endif()
    if(NOT TRACE_CASE_CROP_Y)
        set(TRACE_CASE_CROP_Y 0)
    endif()
    if(NOT TRACE_CASE_CROP_WIDTH)
        set(TRACE_CASE_CROP_WIDTH 0)
    endif()
    if(NOT TRACE_CASE_CROP_HEIGHT)
        set(TRACE_CASE_CROP_HEIGHT 0)
    endif()
    add_test(
            NAME FluorateGLTraceReplay.${CASE_NAME}.${BACKEND}
            COMMAND "${CMAKE_COMMAND}"
            -DTRACE_REPLAY_EXE=$<TARGET_FILE:fluorategl_trace_replay>
            -DFLUORATEGL_LIBRARY=${FLUORATEGL_TRACE_REPLAY_LIBRARY}
            -DTRACE_CASE_NAME=${CASE_NAME}
            -DTRACE_ARCHIVE=${TRACE_CASE_TRACE_ARCHIVE}
            -DTRACE_FILE=${TRACE_CASE_TRACE_FILE}
            -DTRACE_GOLDEN=${TRACE_CASE_GOLDEN}
            -DTRACE_ALTERNATE_GOLDEN=${TRACE_CASE_ALTERNATE_GOLDEN}
            -DTRACE_BACKEND=${BACKEND}
            -DTRACE_TARGET_CALL=${TRACE_CASE_TARGET_CALL}
            -DTRACE_WIDTH=${TRACE_CASE_WIDTH}
            -DTRACE_HEIGHT=${TRACE_CASE_HEIGHT}
            -DTRACE_SSIM_THRESHOLD=${TRACE_CASE_SSIM_THRESHOLD}
            -DTRACE_CROP_X=${TRACE_CASE_CROP_X}
            -DTRACE_CROP_Y=${TRACE_CASE_CROP_Y}
            -DTRACE_CROP_WIDTH=${TRACE_CASE_CROP_WIDTH}
            -DTRACE_CROP_HEIGHT=${TRACE_CASE_CROP_HEIGHT}
            -DTRACE_COHERENT_AS_FLUSH=${TRACE_CASE_COHERENT_AS_FLUSH}
            -DTRACE_OUTPUT_DIR=${CMAKE_CURRENT_BINARY_DIR}/${CASE_NAME}/${BACKEND}
            -DTRACE_ARTIFACT_DIR=${CMAKE_CURRENT_BINARY_DIR}/${CASE_NAME}/actual-images
            -P ${FLUORATEGL_TRACE_ROOT}/run_trace_case.cmake)
    # Single backend: FluorateGL translates desktop GL 3.3 -> GLES 3.1 and
    # renders through its llvmpipe backend; no Vulkan path exists.
    set_tests_properties(FluorateGLTraceReplay.${CASE_NAME}.${BACKEND} PROPERTIES
            ENVIRONMENT "FLUORATEGL_BACKEND=llvmpipe;EGL_PLATFORM=surfaceless;MESA_LOADER_DRIVER_OVERRIDE=llvmpipe;LIBGL_ALWAYS_SOFTWARE=1")
endfunction()

# Generated by trace_cases.py --format cmake; the backend argument is fixed to
# DirectGLES (single-backend lane).
function(add_trace_replay_test_for_backends CASE_NAME)
    add_trace_replay_test(${CASE_NAME} DirectGLES ${ARGN})
endfunction()

set(FLUORATEGL_TRACE_CASES_CMAKE "${CMAKE_CURRENT_BINARY_DIR}/trace_cases.cmake")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${FLUORATEGL_TRACE_ROOT}/trace_cases.json")
execute_process(
        COMMAND ${Python3_EXECUTABLE} "${FLUORATEGL_TRACE_ROOT}/trace_cases.py"
        --format cmake
        --fixture-root "${FLUORATEGL_TRACE_ROOT}/fixtures"
        OUTPUT_FILE "${FLUORATEGL_TRACE_CASES_CMAKE}"
        COMMAND_ERROR_IS_FATAL ANY)
include("${FLUORATEGL_TRACE_CASES_CMAKE}")
