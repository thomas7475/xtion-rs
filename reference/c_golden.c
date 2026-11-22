/* Golden-vector reference harness.
 *
 * This file contains a VERBATIM transcription of the two algorithm cores
 * this crate ports, taken from the sha256-verified OpenNI 2.2.0 source:
 *
 *  - XnShiftToDepthUpdate()  (Source/Drivers/PS1080/DDK/XnShiftToDepth.cpp)
 *  - XnPacked11DepthProcessor::Unpack11to16()
 *      (Source/Drivers/PS1080/Sensor/XnPacked11DepthProcessor.cpp)
 *
 * Only the buffer plumbing (allocation, printing) is new. Field types
 * follow XnShiftToDepth.h: OniDepthPixel = uint16_t, XnFloat = float,
 * XnUInt32 = uint32_t, XnInt32 = int32_t.
 *
 * Build:  gcc -O2 -o c_golden reference/c_golden.c
 * Run:    ./c_golden > reference/golden.txt
 *
 * The Rust test tests/golden.rs parses reference/golden.txt and asserts
 * that the Rust implementations produce identical output.
 *
 * Licensed under the Apache License, Version 2.0; see LICENSE and NOTICE.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* ---------------- shift-to-depth (verbatim) ---------------- */

typedef struct
{
    uint16_t nZeroPlaneDistance;
    float fZeroPlanePixelSize;
    float fEmitterDCmosDistance;
    uint32_t nDeviceMaxShiftValue;
    uint32_t nDeviceMaxDepthValue;
    uint32_t nConstShift;
    uint32_t nPixelSizeFactor;
    uint32_t nParamCoeff;
    uint32_t nShiftScale;
    uint16_t nDepthMinCutOff;
    uint16_t nDepthMaxCutOff;
} XnShiftToDepthConfig;

static void XnShiftToDepthUpdate(const XnShiftToDepthConfig* pConfig,
                                 uint16_t* pShiftToDepthTable,
                                 uint16_t* pDepthToShiftTable)
{
    uint32_t nIndex = 0;
    int16_t nShiftValue = 0;
    double dFixedRefX = 0;
    double dMetric = 0;
    double dDepth = 0;
    double dPlanePixelSize = pConfig->fZeroPlanePixelSize;
    double dPlaneDsr = pConfig->nZeroPlaneDistance;
    double dPlaneDcl = pConfig->fEmitterDCmosDistance;
    int32_t nConstShift = pConfig->nParamCoeff * pConfig->nConstShift;

    dPlanePixelSize *= pConfig->nPixelSizeFactor;
    nConstShift /= pConfig->nPixelSizeFactor;

    memset(pShiftToDepthTable, 0,
           (pConfig->nDeviceMaxShiftValue + 1) * sizeof(uint16_t));
    memset(pDepthToShiftTable, 0,
           (pConfig->nDeviceMaxDepthValue + 1) * sizeof(uint16_t));

    uint16_t nLastDepth = 0;
    uint16_t nLastIndex = 0;

    uint32_t nMaxDepth = pConfig->nDeviceMaxDepthValue < pConfig->nDepthMaxCutOff
                             ? pConfig->nDeviceMaxDepthValue
                             : pConfig->nDepthMaxCutOff;

    for (nIndex = 1; nIndex < pConfig->nDeviceMaxShiftValue; nIndex++)
    {
        nShiftValue = (int16_t)nIndex;

        dFixedRefX = (double)(nShiftValue - nConstShift) / (double)pConfig->nParamCoeff;
        dFixedRefX -= 0.375;
        dMetric = dFixedRefX * dPlanePixelSize;
        dDepth = pConfig->nShiftScale * ((dMetric * dPlaneDsr / (dPlaneDcl - dMetric)) + dPlaneDsr);

        if ((dDepth > pConfig->nDepthMinCutOff) && (dDepth < nMaxDepth))
        {
            pShiftToDepthTable[nIndex] = (uint16_t)dDepth;

            for (uint16_t i = nLastDepth; i < dDepth; i++)
                pDepthToShiftTable[i] = nLastIndex;

            nLastIndex = (uint16_t)nIndex;
            nLastDepth = (uint16_t)dDepth;
        }
    }

    for (uint16_t i = nLastDepth; i <= pConfig->nDeviceMaxDepthValue; i++)
        pDepthToShiftTable[i] = nLastIndex;
}

/* ---------------- packed 11 (verbatim) ---------------- */

#define XN_ON_BITS(count) ((1 << (count)) - 1)
#define XN_CREATE_MASK(count, offset) (XN_ON_BITS(count) << offset)
#define XN_TAKE_BITS(source, count, offset) \
    ((source & XN_CREATE_MASK(count, offset)) >> offset)

static void Unpack11to16(const uint8_t* pcInput, size_t nInputSize, uint16_t* pnOutput)
{
    size_t nElements = nInputSize / 11; /* floored */

    for (size_t nElem = 0; nElem < nElements; ++nElem)
    {
        const uint8_t* p = pcInput + nElem * 11;

        pnOutput[nElem * 8 + 0] = (uint16_t)((XN_TAKE_BITS(p[0], 8, 0) << 3) | XN_TAKE_BITS(p[1], 3, 5));
        pnOutput[nElem * 8 + 1] = (uint16_t)((XN_TAKE_BITS(p[1], 5, 0) << 6) | XN_TAKE_BITS(p[2], 6, 2));
        pnOutput[nElem * 8 + 2] = (uint16_t)((XN_TAKE_BITS(p[2], 2, 0) << 9) | (XN_TAKE_BITS(p[3], 8, 0) << 1) | XN_TAKE_BITS(p[4], 1, 7));
        pnOutput[nElem * 8 + 3] = (uint16_t)((XN_TAKE_BITS(p[4], 7, 0) << 4) | XN_TAKE_BITS(p[5], 4, 4));
        pnOutput[nElem * 8 + 4] = (uint16_t)((XN_TAKE_BITS(p[5], 4, 0) << 7) | XN_TAKE_BITS(p[6], 7, 1));
        pnOutput[nElem * 8 + 5] = (uint16_t)((XN_TAKE_BITS(p[6], 1, 0) << 10) | (XN_TAKE_BITS(p[7], 8, 0) << 2) | XN_TAKE_BITS(p[8], 2, 6));
        pnOutput[nElem * 8 + 6] = (uint16_t)((XN_TAKE_BITS(p[8], 6, 0) << 5) | XN_TAKE_BITS(p[9], 5, 3));
        pnOutput[nElem * 8 + 7] = (uint16_t)((XN_TAKE_BITS(p[9], 3, 0) << 8) | XN_TAKE_BITS(p[10], 8, 0));
    }
}

/* ---------------- golden output ---------------- */

static void emit_s2d(const char* name, const XnShiftToDepthConfig* cfg)
{
    uint16_t* s2d = calloc(cfg->nDeviceMaxShiftValue + 1, sizeof(uint16_t));
    uint16_t* d2s = calloc(cfg->nDeviceMaxDepthValue + 1, sizeof(uint16_t));
    XnShiftToDepthUpdate(cfg, s2d, d2s);

    printf("S2D %s %u %.9g %.9g %u %u %u %u %u %u %u %u\n", name,
           cfg->nZeroPlaneDistance, cfg->fZeroPlanePixelSize,
           cfg->fEmitterDCmosDistance, cfg->nDeviceMaxShiftValue,
           cfg->nDeviceMaxDepthValue, cfg->nConstShift, cfg->nPixelSizeFactor,
           cfg->nParamCoeff, cfg->nShiftScale, cfg->nDepthMinCutOff,
           cfg->nDepthMaxCutOff);

    printf("S2D_TABLE");
    for (uint32_t i = 0; i <= cfg->nDeviceMaxShiftValue; i++)
        printf(" %u", s2d[i]);
    printf("\n");

    printf("D2S_TABLE");
    for (uint32_t i = 0; i <= cfg->nDeviceMaxDepthValue; i++)
        printf(" %u", d2s[i]);
    printf("\n");

    free(s2d);
    free(d2s);
}

static void emit_packed11(uint32_t seed)
{
    /* a deterministic pseudo-random stream so the Rust test can compare
     * inputs and outputs byte for byte */
    uint8_t buf[11 * 512];
    srand(seed);
    for (size_t i = 0; i < sizeof(buf); i++)
        buf[i] = (uint8_t)(rand() & 0xFF);

    uint16_t out[8 * 512];
    Unpack11to16(buf, sizeof(buf), out);

    printf("PACKED11 %u %zu\n", seed, sizeof(buf));
    for (size_t g = 0; g < 512; g++)
    {
        printf("P11");
        for (int i = 0; i < 11; i++)
            printf(" %02x", buf[g * 11 + i]);
        for (int i = 0; i < 8; i++)
            printf(" %u", out[g * 8 + i]);
        printf("\n");
    }
}

int main(void)
{
    /* config 1: the crate's defaults */
    XnShiftToDepthConfig c1 = {
        .nZeroPlaneDistance = 120,
        .fZeroPlanePixelSize = 0.1052f,
        .fEmitterDCmosDistance = 7.5f,
        .nDeviceMaxShiftValue = 1024,
        .nDeviceMaxDepthValue = 10000,
        .nConstShift = 300,
        .nPixelSizeFactor = 1,
        .nParamCoeff = 4,
        .nShiftScale = 10,
        .nDepthMinCutOff = 0,
        .nDepthMaxCutOff = 10000,
    };
    emit_s2d("default", &c1);

    /* config 2: alternate calibration with cutoffs and a factor */
    XnShiftToDepthConfig c2 = {
        .nZeroPlaneDistance = 145,
        .fZeroPlanePixelSize = 0.0701f,
        .fEmitterDCmosDistance = 6.9f,
        .nDeviceMaxShiftValue = 1024,
        .nDeviceMaxDepthValue = 8000,
        .nConstShift = 361,
        .nPixelSizeFactor = 2,
        .nParamCoeff = 4,
        .nShiftScale = 10,
        .nDepthMinCutOff = 50,
        .nDepthMaxCutOff = 6000,
    };
    emit_s2d("alternate", &c2);

    emit_packed11(42);
    return 0;
}
