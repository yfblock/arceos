#ifndef __CVI_JPEG_CFG_H__
#define __CVI_JPEG_CFG_H__

#define BM_MASK_ERR     0x1
#define BM_MASK_FLOW    0x2
#define BM_MASK_MEM     0x4
#define BM_MASK_TRACE   0x10
#define BM_MASK_PERF    0x20
#define BM_MASK_ALL     0xFFFF

#define BM_DBG_ERR(msg, ...)
#define BM_DBG_FLOW(msg, ...)
#define BM_DBG_MEM(msg, ...)
#define BM_DBG_TRACE(msg, ...)
#define BM_DBG_PERF(msg, ...)

#define JPEG_CODEC_INTR_NUM  75

#endif
