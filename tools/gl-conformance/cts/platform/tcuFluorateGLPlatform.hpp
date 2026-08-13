#ifndef _TCUFLUORATEGLPLATFORM_HPP
#define _TCUFLUORATEGLPLATFORM_HPP
/*-------------------------------------------------------------------------
 * dEQP platform port for FluorateGL on desktop Linux (Android legacy path
 * kept behind __ANDROID__ guards, see the .cpp)
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 *//*!
 * \file
 * \brief FluorateGL platform - drives libfluorategl.so's own EGL from a
 *        plain desktop process, with no system EGL involved.
 *//*--------------------------------------------------------------------*/

#include "tcuDefs.hpp"

namespace tcu
{
class Platform;
}

tcu::Platform *createPlatform(void);

#endif // _TCUFLUORATEGLPLATFORM_HPP
