# 固定管线函数待办清单（Fixed Pipeline）

## 状态：待办（记录于 2026-08-08，暂不实现）

## 为什么不做
- **北极星原则**：本项目翻译目标是**桌面 GL 3.3 core profile**——固定管线（immediate mode /
  矩阵栈 / display list / 客户端数组）在 core profile 中已被移除，MC 1.21 / LWJGL 3 / Sodium
  均不使用这些函数
- **GLES 无对应**：固定管线语义无法映射到 GLES（无矩阵栈、无 glBegin/glEnd、无 client state）
- **安全**：不导出 = eglGetProcAddress 返回 null = LWJGL capabilities 标记 false；应用调用时才崩溃，
  而依赖固定管线的应用在 core profile 下本就不该运行——保持不导出是「诚实」行为
- 若将来需兼容旧应用（GL 1.x-2.1 固定管线渲染器），再评估模拟方案（软件矩阵栈 + 立即模式
  顶点缓冲转换）

## 数量与来源
- 函数总数：**715**
- 提取逻辑：tools/gen_stub_exports.py 的 `FIXED_PIPELINE_PREFIXES` + `is_fixed_pipeline()`
  （与生成器排除口径一致；例外：glVertexAttrib*/glVertexArray*/glVertexBinding*/P 系列属 core 非固定管线）
- 签名源：glcorearb.h（1257）+ MobileGlues gles.h GL_FUNC_TYPEDEF + MG STUB/NATIVE 宏

## 分组统计（按前缀，共 95 组）

- `glMultiTexCoord*` × 89
- `glVertex*` × 82
- `glTexCoord*` × 70
- `glWindowPos*` × 56
- `glColor*` × 53
- `glSecondaryColor*` × 37
- `glRasterPos*` × 30
- `glNormal*` × 26
- `glIndex*` × 15
- `glFogCoord*` × 13
- `glEvalCoord*` × 12
- `glBegin*` × 11
- `glEnd*` × 10
- `glRect*` × 10
- `glTexGen*` × 8
- `glColorTable*` × 7
- `glFog*` × 7
- `glGetMap*` × 7
- `glLight*` × 7
- `glMatrixMult*` × 7
- `glLightModel*` × 6
- `glMaterial*` × 6
- `glColorPointer*` × 4
- `glGetPixelMap*` × 4
- `glGetTexGen*` × 4
- `glMatrixLoad*` × 4
- `glNormalPointer*` × 4
- `glPixelMap*` × 4
- `glTexCoordPointer*` × 4
- `glVertexPointer*` × 4
- `glClipPlane*` × 3
- `glDisableClientState*` × 3
- `glEdgeFlagPointer*` × 3
- `glEnableClientState*` × 3
- `glFrustum*` × 3
- `glGetClipPlane*` × 3
- `glIndexPointer*` × 3
- `glLoadMatrix*` × 3
- `glMap1*` × 3
- `glMap2*` × 3
- `glMatrixLoadTranspose*` × 3
- `glMultMatrix*` × 3
- `glOrtho*` × 3
- `glPixelTransfer*` × 3
- `glRotate*` × 3
- `glScale*` × 3
- `glTranslate*` × 3
- `glAccum*` × 2
- `glArrayElement*` × 2
- `glBitmap*` × 2
- `glCallList*` × 2
- `glClearAccum*` × 2
- `glClientActiveTexture*` × 2
- `glColorSubTable*` × 2
- `glEdgeFlag*` × 2
- `glEvalMesh*` × 2
- `glEvalPoint*` × 2
- `glFeedbackBuffer*` × 2
- `glMatrixRotate*` × 2
- `glMatrixScale*` × 2
- `glMatrixTranslate*` × 2
- `glPassThrough*` × 2
- `glClearIndex*` × 1
- `glColorFormat*` × 1
- `glCopyPixels*` × 1
- `glDeleteLists*` × 1
- `glDrawPixels*` × 1
- `glEdgeFlagFormat*` × 1
- `glFogCoordFormat*` × 1
- `glGenLists*` × 1
- `glGetPointerIndexedvEXT*` × 1
- `glGetPolygonStipple*` × 1
- `glIndexFormat*` × 1
- `glInterleavedArrays*` × 1
- `glIsList*` × 1
- `glLineStipple*` × 1
- `glListBase*` × 1
- `glLoadIdentity*` × 1
- `glLoadName*` × 1
- `glLockArrays*` × 1
- `glMatrixFrustum*` × 1
- `glMatrixLoadIdentity*` × 1
- `glMatrixMode*` × 1
- `glMatrixOrtho*` × 1
- `glMatrixPop*` × 1
- `glMatrixPush*` × 1
- `glNewList*` × 1
- `glNormalFormat*` × 1
- `glPolygonStipple*` × 1
- `glPushName*` × 1
- `glSecondaryColorFormat*` × 1
- `glSelectBuffer*` × 1
- `glTexCoordFormat*` × 1
- `glUnlockArrays*` × 1
- `glVertexFormat*` × 1

## 完整清单（按字母序分组）

```
# --- glAccum* ---
glAccum
glAccumxOES
# --- glArrayElement* ---
glArrayElement
glArrayElementEXT
# --- glBegin* ---
glBegin
glBeginConditionalRenderNV
glBeginConditionalRenderNVX
glBeginFragmentShaderATI
glBeginOcclusionQueryNV
glBeginPerfMonitorAMD
glBeginPerfQueryINTEL
glBeginQueryIndexed
glBeginTransformFeedbackNV
glBeginVertexShaderEXT
glBeginVideoCaptureNV
# --- glBitmap* ---
glBitmap
glBitmapxOES
# --- glCallList* ---
glCallList
glCallLists
# --- glClearAccum* ---
glClearAccum
glClearAccumxOES
# --- glClearIndex* ---
glClearIndex
# --- glClientActiveTexture* ---
glClientActiveTexture
glClientActiveTextureARB
# --- glClipPlane* ---
glClipPlane
glClipPlanefOES
glClipPlanexOES
# --- glColor* ---
glColor3b
glColor3bv
glColor3d
glColor3dv
glColor3f
glColor3fVertex3fSUN
glColor3fVertex3fvSUN
glColor3fv
glColor3hNV
glColor3hvNV
glColor3i
glColor3iv
glColor3s
glColor3sv
glColor3ub
glColor3ubv
glColor3ui
glColor3uiv
glColor3us
glColor3usv
glColor3xOES
glColor3xvOES
glColor4b
glColor4bv
glColor4d
glColor4dv
glColor4f
glColor4fNormal3fVertex3fSUN
glColor4fNormal3fVertex3fvSUN
glColor4fv
glColor4hNV
glColor4hvNV
glColor4i
glColor4iv
glColor4s
glColor4sv
glColor4ub
glColor4ubVertex2fSUN
glColor4ubVertex2fvSUN
glColor4ubVertex3fSUN
glColor4ubVertex3fvSUN
glColor4ubv
glColor4ui
glColor4uiv
glColor4us
glColor4usv
glColor4xOES
glColor4xvOES
# --- glColorFormat* ---
glColorFormatNV
# --- glColor* ---
glColorFragmentOp1ATI
glColorFragmentOp2ATI
glColorFragmentOp3ATI
glColorMaskIndexedEXT
glColorMaterial
# --- glColorPointer* ---
glColorPointer
glColorPointerEXT
glColorPointerListIBM
glColorPointervINTEL
# --- glColorSubTable* ---
glColorSubTable
glColorSubTableEXT
# --- glColorTable* ---
glColorTable
glColorTableEXT
glColorTableParameterfv
glColorTableParameterfvSGI
glColorTableParameteriv
glColorTableParameterivSGI
glColorTableSGI
# --- glCopyPixels* ---
glCopyPixels
# --- glDeleteLists* ---
glDeleteLists
# --- glDisableClientState* ---
glDisableClientState
glDisableClientStateIndexedEXT
glDisableClientStateiEXT
# --- glDrawPixels* ---
glDrawPixels
# --- glEdgeFlag* ---
glEdgeFlag
# --- glEdgeFlagFormat* ---
glEdgeFlagFormatNV
# --- glEdgeFlagPointer* ---
glEdgeFlagPointer
glEdgeFlagPointerEXT
glEdgeFlagPointerListIBM
# --- glEdgeFlag* ---
glEdgeFlagv
# --- glEnableClientState* ---
glEnableClientState
glEnableClientStateIndexedEXT
glEnableClientStateiEXT
# --- glEnd* ---
glEndConditionalRenderNV
glEndConditionalRenderNVX
glEndFragmentShaderATI
glEndOcclusionQueryNV
glEndPerfMonitorAMD
glEndPerfQueryINTEL
glEndQueryIndexed
glEndTransformFeedbackNV
glEndVertexShaderEXT
glEndVideoCaptureNV
# --- glEvalCoord* ---
glEvalCoord1d
glEvalCoord1dv
glEvalCoord1f
glEvalCoord1fv
glEvalCoord1xOES
glEvalCoord1xvOES
glEvalCoord2d
glEvalCoord2dv
glEvalCoord2f
glEvalCoord2fv
glEvalCoord2xOES
glEvalCoord2xvOES
# --- glEvalMesh* ---
glEvalMesh1
glEvalMesh2
# --- glEvalPoint* ---
glEvalPoint1
glEvalPoint2
# --- glFeedbackBuffer* ---
glFeedbackBuffer
glFeedbackBufferxOES
# --- glFogCoordFormat* ---
glFogCoordFormatNV
# --- glFogCoord* ---
glFogCoordPointer
glFogCoordPointerEXT
glFogCoordPointerListIBM
glFogCoordd
glFogCoorddEXT
glFogCoorddv
glFogCoorddvEXT
glFogCoordf
glFogCoordfEXT
glFogCoordfv
glFogCoordfvEXT
glFogCoordhNV
glFogCoordhvNV
# --- glFog* ---
glFogFuncSGIS
glFogf
glFogfv
glFogi
glFogiv
glFogxOES
glFogxvOES
# --- glFrustum* ---
glFrustum
glFrustumfOES
glFrustumxOES
# --- glGenLists* ---
glGenLists
# --- glGetClipPlane* ---
glGetClipPlane
glGetClipPlanefOES
glGetClipPlanexOES
# --- glGetMap* ---
glGetMapControlPointsNV
glGetMapParameterfvNV
glGetMapParameterivNV
glGetMapdv
glGetMapfv
glGetMapiv
glGetMapxvOES
# --- glGetPixelMap* ---
glGetPixelMapfv
glGetPixelMapuiv
glGetPixelMapusv
glGetPixelMapxv
# --- glGetPointerIndexedvEXT* ---
glGetPointerIndexedvEXT
# --- glGetPolygonStipple* ---
glGetPolygonStipple
# --- glGetTexGen* ---
glGetTexGendv
glGetTexGenfv
glGetTexGeniv
glGetTexGenxvOES
# --- glIndexFormat* ---
glIndexFormatNV
# --- glIndex* ---
glIndexFuncEXT
glIndexMask
glIndexMaterialEXT
# --- glIndexPointer* ---
glIndexPointer
glIndexPointerEXT
glIndexPointerListIBM
# --- glIndex* ---
glIndexd
glIndexdv
glIndexf
glIndexfv
glIndexi
glIndexiv
glIndexs
glIndexsv
glIndexub
glIndexubv
glIndexxOES
glIndexxvOES
# --- glInterleavedArrays* ---
glInterleavedArrays
# --- glIsList* ---
glIsList
# --- glLight* ---
glLightEnviSGIX
# --- glLightModel* ---
glLightModelf
glLightModelfv
glLightModeli
glLightModeliv
glLightModelxOES
glLightModelxvOES
# --- glLight* ---
glLightf
glLightfv
glLighti
glLightiv
glLightxOES
glLightxvOES
# --- glLineStipple* ---
glLineStipple
# --- glListBase* ---
glListBase
# --- glLoadIdentity* ---
glLoadIdentityDeformationMapSGIX
# --- glLoadMatrix* ---
glLoadMatrixd
glLoadMatrixf
glLoadMatrixxOES
# --- glLoadName* ---
glLoadName
# --- glLockArrays* ---
glLockArraysEXT
# --- glMap1* ---
glMap1d
glMap1f
glMap1xOES
# --- glMap2* ---
glMap2d
glMap2f
glMap2xOES
# --- glMaterial* ---
glMaterialf
glMaterialfv
glMateriali
glMaterialiv
glMaterialxOES
glMaterialxvOES
# --- glMatrixFrustum* ---
glMatrixFrustumEXT
# --- glMatrixLoad* ---
glMatrixLoad3x2fNV
glMatrixLoad3x3fNV
# --- glMatrixLoadIdentity* ---
glMatrixLoadIdentityEXT
# --- glMatrixLoadTranspose* ---
glMatrixLoadTranspose3x3fNV
glMatrixLoadTransposedEXT
glMatrixLoadTransposefEXT
# --- glMatrixLoad* ---
glMatrixLoaddEXT
glMatrixLoadfEXT
# --- glMatrixMode* ---
glMatrixMode
# --- glMatrixMult* ---
glMatrixMult3x2fNV
glMatrixMult3x3fNV
glMatrixMultTranspose3x3fNV
glMatrixMultTransposedEXT
glMatrixMultTransposefEXT
glMatrixMultdEXT
glMatrixMultfEXT
# --- glMatrixOrtho* ---
glMatrixOrthoEXT
# --- glMatrixPop* ---
glMatrixPopEXT
# --- glMatrixPush* ---
glMatrixPushEXT
# --- glMatrixRotate* ---
glMatrixRotatedEXT
glMatrixRotatefEXT
# --- glMatrixScale* ---
glMatrixScaledEXT
glMatrixScalefEXT
# --- glMatrixTranslate* ---
glMatrixTranslatedEXT
glMatrixTranslatefEXT
# --- glMultMatrix* ---
glMultMatrixd
glMultMatrixf
glMultMatrixxOES
# --- glMultiTexCoord* ---
glMultiTexCoord1bOES
glMultiTexCoord1bvOES
glMultiTexCoord1d
glMultiTexCoord1dARB
glMultiTexCoord1dv
glMultiTexCoord1dvARB
glMultiTexCoord1f
glMultiTexCoord1fARB
glMultiTexCoord1fv
glMultiTexCoord1fvARB
glMultiTexCoord1hNV
glMultiTexCoord1hvNV
glMultiTexCoord1i
glMultiTexCoord1iARB
glMultiTexCoord1iv
glMultiTexCoord1ivARB
glMultiTexCoord1s
glMultiTexCoord1sARB
glMultiTexCoord1sv
glMultiTexCoord1svARB
glMultiTexCoord1xOES
glMultiTexCoord1xvOES
glMultiTexCoord2bOES
glMultiTexCoord2bvOES
glMultiTexCoord2d
glMultiTexCoord2dARB
glMultiTexCoord2dv
glMultiTexCoord2dvARB
glMultiTexCoord2f
glMultiTexCoord2fARB
glMultiTexCoord2fv
glMultiTexCoord2fvARB
glMultiTexCoord2hNV
glMultiTexCoord2hvNV
glMultiTexCoord2i
glMultiTexCoord2iARB
glMultiTexCoord2iv
glMultiTexCoord2ivARB
glMultiTexCoord2s
glMultiTexCoord2sARB
glMultiTexCoord2sv
glMultiTexCoord2svARB
glMultiTexCoord2xOES
glMultiTexCoord2xvOES
glMultiTexCoord3bOES
glMultiTexCoord3bvOES
glMultiTexCoord3d
glMultiTexCoord3dARB
glMultiTexCoord3dv
glMultiTexCoord3dvARB
glMultiTexCoord3f
glMultiTexCoord3fARB
glMultiTexCoord3fv
glMultiTexCoord3fvARB
glMultiTexCoord3hNV
glMultiTexCoord3hvNV
glMultiTexCoord3i
glMultiTexCoord3iARB
glMultiTexCoord3iv
glMultiTexCoord3ivARB
glMultiTexCoord3s
glMultiTexCoord3sARB
glMultiTexCoord3sv
glMultiTexCoord3svARB
glMultiTexCoord3xOES
glMultiTexCoord3xvOES
glMultiTexCoord4bOES
glMultiTexCoord4bvOES
glMultiTexCoord4d
glMultiTexCoord4dARB
glMultiTexCoord4dv
glMultiTexCoord4dvARB
glMultiTexCoord4f
glMultiTexCoord4fARB
glMultiTexCoord4fv
glMultiTexCoord4fvARB
glMultiTexCoord4hNV
glMultiTexCoord4hvNV
glMultiTexCoord4i
glMultiTexCoord4iARB
glMultiTexCoord4iv
glMultiTexCoord4ivARB
glMultiTexCoord4s
glMultiTexCoord4sARB
glMultiTexCoord4sv
glMultiTexCoord4svARB
glMultiTexCoord4xOES
glMultiTexCoord4xvOES
glMultiTexCoordPointerEXT
# --- glNewList* ---
glNewList
# --- glNormal* ---
glNormal3b
glNormal3bv
glNormal3d
glNormal3dv
glNormal3f
glNormal3fVertex3fSUN
glNormal3fVertex3fvSUN
glNormal3fv
glNormal3hNV
glNormal3hvNV
glNormal3i
glNormal3iv
glNormal3s
glNormal3sv
glNormal3xOES
glNormal3xvOES
# --- glNormalFormat* ---
glNormalFormatNV
# --- glNormalPointer* ---
glNormalPointer
glNormalPointerEXT
glNormalPointerListIBM
glNormalPointervINTEL
# --- glNormal* ---
glNormalStream3bATI
glNormalStream3bvATI
glNormalStream3dATI
glNormalStream3dvATI
glNormalStream3fATI
glNormalStream3fvATI
glNormalStream3iATI
glNormalStream3ivATI
glNormalStream3sATI
glNormalStream3svATI
# --- glOrtho* ---
glOrtho
glOrthofOES
glOrthoxOES
# --- glPassThrough* ---
glPassThrough
glPassThroughxOES
# --- glPixelMap* ---
glPixelMapfv
glPixelMapuiv
glPixelMapusv
glPixelMapx
# --- glPixelTransfer* ---
glPixelTransferf
glPixelTransferi
glPixelTransferxOES
# --- glPolygonStipple* ---
glPolygonStipple
# --- glPushName* ---
glPushName
# --- glRasterPos* ---
glRasterPos2d
glRasterPos2dv
glRasterPos2f
glRasterPos2fv
glRasterPos2i
glRasterPos2iv
glRasterPos2s
glRasterPos2sv
glRasterPos2xOES
glRasterPos2xvOES
glRasterPos3d
glRasterPos3dv
glRasterPos3f
glRasterPos3fv
glRasterPos3i
glRasterPos3iv
glRasterPos3s
glRasterPos3sv
glRasterPos3xOES
glRasterPos3xvOES
glRasterPos4d
glRasterPos4dv
glRasterPos4f
glRasterPos4fv
glRasterPos4i
glRasterPos4iv
glRasterPos4s
glRasterPos4sv
glRasterPos4xOES
glRasterPos4xvOES
# --- glRect* ---
glRectd
glRectdv
glRectf
glRectfv
glRecti
glRectiv
glRects
glRectsv
glRectxOES
glRectxvOES
# --- glRotate* ---
glRotated
glRotatef
glRotatexOES
# --- glScale* ---
glScaled
glScalef
glScalexOES
# --- glSecondaryColor* ---
glSecondaryColor3b
glSecondaryColor3bEXT
glSecondaryColor3bv
glSecondaryColor3bvEXT
glSecondaryColor3d
glSecondaryColor3dEXT
glSecondaryColor3dv
glSecondaryColor3dvEXT
glSecondaryColor3f
glSecondaryColor3fEXT
glSecondaryColor3fv
glSecondaryColor3fvEXT
glSecondaryColor3hNV
glSecondaryColor3hvNV
glSecondaryColor3i
glSecondaryColor3iEXT
glSecondaryColor3iv
glSecondaryColor3ivEXT
glSecondaryColor3s
glSecondaryColor3sEXT
glSecondaryColor3sv
glSecondaryColor3svEXT
glSecondaryColor3ub
glSecondaryColor3ubEXT
glSecondaryColor3ubv
glSecondaryColor3ubvEXT
glSecondaryColor3ui
glSecondaryColor3uiEXT
glSecondaryColor3uiv
glSecondaryColor3uivEXT
glSecondaryColor3us
glSecondaryColor3usEXT
glSecondaryColor3usv
glSecondaryColor3usvEXT
# --- glSecondaryColorFormat* ---
glSecondaryColorFormatNV
# --- glSecondaryColor* ---
glSecondaryColorPointer
glSecondaryColorPointerEXT
glSecondaryColorPointerListIBM
# --- glSelectBuffer* ---
glSelectBuffer
# --- glTexCoord* ---
glTexCoord1bOES
glTexCoord1bvOES
glTexCoord1d
glTexCoord1dv
glTexCoord1f
glTexCoord1fv
glTexCoord1hNV
glTexCoord1hvNV
glTexCoord1i
glTexCoord1iv
glTexCoord1s
glTexCoord1sv
glTexCoord1xOES
glTexCoord1xvOES
glTexCoord2bOES
glTexCoord2bvOES
glTexCoord2d
glTexCoord2dv
glTexCoord2f
glTexCoord2fColor3fVertex3fSUN
glTexCoord2fColor3fVertex3fvSUN
glTexCoord2fColor4fNormal3fVertex3fSUN
glTexCoord2fColor4fNormal3fVertex3fvSUN
glTexCoord2fColor4ubVertex3fSUN
glTexCoord2fColor4ubVertex3fvSUN
glTexCoord2fNormal3fVertex3fSUN
glTexCoord2fNormal3fVertex3fvSUN
glTexCoord2fVertex3fSUN
glTexCoord2fVertex3fvSUN
glTexCoord2fv
glTexCoord2hNV
glTexCoord2hvNV
glTexCoord2i
glTexCoord2iv
glTexCoord2s
glTexCoord2sv
glTexCoord2xOES
glTexCoord2xvOES
glTexCoord3bOES
glTexCoord3bvOES
glTexCoord3d
glTexCoord3dv
glTexCoord3f
glTexCoord3fv
glTexCoord3hNV
glTexCoord3hvNV
glTexCoord3i
glTexCoord3iv
glTexCoord3s
glTexCoord3sv
glTexCoord3xOES
glTexCoord3xvOES
glTexCoord4bOES
glTexCoord4bvOES
glTexCoord4d
glTexCoord4dv
glTexCoord4f
glTexCoord4fColor4fNormal3fVertex4fSUN
glTexCoord4fColor4fNormal3fVertex4fvSUN
glTexCoord4fVertex4fSUN
glTexCoord4fVertex4fvSUN
glTexCoord4fv
glTexCoord4hNV
glTexCoord4hvNV
glTexCoord4i
glTexCoord4iv
glTexCoord4s
glTexCoord4sv
glTexCoord4xOES
glTexCoord4xvOES
# --- glTexCoordFormat* ---
glTexCoordFormatNV
# --- glTexCoordPointer* ---
glTexCoordPointer
glTexCoordPointerEXT
glTexCoordPointerListIBM
glTexCoordPointervINTEL
# --- glTexGen* ---
glTexGend
glTexGendv
glTexGenf
glTexGenfv
glTexGeni
glTexGeniv
glTexGenxOES
glTexGenxvOES
# --- glTranslate* ---
glTranslated
glTranslatef
glTranslatexOES
# --- glUnlockArrays* ---
glUnlockArraysEXT
# --- glVertex* ---
glVertex2bOES
glVertex2bvOES
glVertex2d
glVertex2dv
glVertex2f
glVertex2fv
glVertex2hNV
glVertex2hvNV
glVertex2i
glVertex2iv
glVertex2s
glVertex2sv
glVertex2xOES
glVertex2xvOES
glVertex3bOES
glVertex3bvOES
glVertex3d
glVertex3dv
glVertex3f
glVertex3fv
glVertex3hNV
glVertex3hvNV
glVertex3i
glVertex3iv
glVertex3s
glVertex3sv
glVertex3xOES
glVertex3xvOES
glVertex4bOES
glVertex4bvOES
glVertex4d
glVertex4dv
glVertex4f
glVertex4fv
glVertex4hNV
glVertex4hvNV
glVertex4i
glVertex4iv
glVertex4s
glVertex4sv
glVertex4xOES
glVertex4xvOES
glVertexBlendARB
glVertexBlendEnvfATI
glVertexBlendEnviATI
# --- glVertexFormat* ---
glVertexFormatNV
# --- glVertexPointer* ---
glVertexPointer
glVertexPointerEXT
glVertexPointerListIBM
glVertexPointervINTEL
# --- glVertex* ---
glVertexStream1dATI
glVertexStream1dvATI
glVertexStream1fATI
glVertexStream1fvATI
glVertexStream1iATI
glVertexStream1ivATI
glVertexStream1sATI
glVertexStream1svATI
glVertexStream2dATI
glVertexStream2dvATI
glVertexStream2fATI
glVertexStream2fvATI
glVertexStream2iATI
glVertexStream2ivATI
glVertexStream2sATI
glVertexStream2svATI
glVertexStream3dATI
glVertexStream3dvATI
glVertexStream3fATI
glVertexStream3fvATI
glVertexStream3iATI
glVertexStream3ivATI
glVertexStream3sATI
glVertexStream3svATI
glVertexStream4dATI
glVertexStream4dvATI
glVertexStream4fATI
glVertexStream4fvATI
glVertexStream4iATI
glVertexStream4ivATI
glVertexStream4sATI
glVertexStream4svATI
glVertexWeightPointerEXT
glVertexWeightfEXT
glVertexWeightfvEXT
glVertexWeighthNV
glVertexWeighthvNV
# --- glWindowPos* ---
glWindowPos2d
glWindowPos2dARB
glWindowPos2dMESA
glWindowPos2dv
glWindowPos2dvARB
glWindowPos2dvMESA
glWindowPos2f
glWindowPos2fARB
glWindowPos2fMESA
glWindowPos2fv
glWindowPos2fvARB
glWindowPos2fvMESA
glWindowPos2i
glWindowPos2iARB
glWindowPos2iMESA
glWindowPos2iv
glWindowPos2ivARB
glWindowPos2ivMESA
glWindowPos2s
glWindowPos2sARB
glWindowPos2sMESA
glWindowPos2sv
glWindowPos2svARB
glWindowPos2svMESA
glWindowPos3d
glWindowPos3dARB
glWindowPos3dMESA
glWindowPos3dv
glWindowPos3dvARB
glWindowPos3dvMESA
glWindowPos3f
glWindowPos3fARB
glWindowPos3fMESA
glWindowPos3fv
glWindowPos3fvARB
glWindowPos3fvMESA
glWindowPos3i
glWindowPos3iARB
glWindowPos3iMESA
glWindowPos3iv
glWindowPos3ivARB
glWindowPos3ivMESA
glWindowPos3s
glWindowPos3sARB
glWindowPos3sMESA
glWindowPos3sv
glWindowPos3svARB
glWindowPos3svMESA
glWindowPos4dMESA
glWindowPos4dvMESA
glWindowPos4fMESA
glWindowPos4fvMESA
glWindowPos4iMESA
glWindowPos4ivMESA
glWindowPos4sMESA
glWindowPos4svMESA
```

## 备注
- 若决定实现，建议：软矩阵栈 + glBegin/glEnd 顶点收集 → 合成 VBO/VAO 提交 GLES；
  display list 可模拟为命令缓冲重放；优先级低（无真实应用场景）
