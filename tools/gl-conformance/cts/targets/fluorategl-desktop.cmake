#-------------------------------------------------------------------------
# VK-GL-CTS target: FluorateGL on desktop Linux
#
# Builds glcts as a normal host executable that reaches OpenGL exclusively
# through libfluorategl.so, loaded at runtime via the eglw dynamic wrapper.
# Nothing here links libEGL or libGL: a conformance result must be
# unambiguously FluorateGL's, never the system GL stack's.
#
# The platform port only offers pbuffer surfaces on desktop.
#-------------------------------------------------------------------------

message("*** Using FluorateGL desktop target")

set(DEQP_TARGET_NAME "FluorateGL")

# EGL comes from libfluorategl.so via the eglw dynamic wrapper, so the support
# flag is on but no import library is supplied.
set(DEQP_SUPPORT_EGL ON)
set(DEQP_EGL_LIBRARIES)
set(DEQP_GLES2_LIBRARIES)
set(DEQP_GLES3_LIBRARIES)

set(TCUTIL_PLATFORM_SRCS
	fluorategl/tcuFluorateGLPlatform.cpp
	fluorategl/tcuFluorateGLPlatform.hpp
	)

list(APPEND TCUTIL_PLATFORM_LIBS dl pthread)
