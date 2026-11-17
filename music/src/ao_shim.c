/* Minimal wrapper around the SigmaStar MI_AO audio output API used by the
 * Miyoo Mini firmware: open device 0 / channel 0 as 16-bit PCM and push
 * interleaved frames. Settings follow the Miyoo Mini SDL audio driver. */
#include <string.h>
#include <unistd.h>
#include <mi_sys.h>
#include <mi_ao.h>

static MI_AUDIO_Attr_t attr;
static int opened;

int ao_open(unsigned rate, unsigned channels, unsigned frame_samples)
{
    MI_S32 r;
    if ((r = MI_SYS_Init()) != MI_SUCCESS) return -1;
    memset(&attr, 0, sizeof attr);
    attr.eSamplerate = (MI_AUDIO_SampleRate_e)rate;
    attr.eBitwidth = E_MI_AUDIO_BIT_WIDTH_16;
    attr.eWorkmode = E_MI_AUDIO_MODE_I2S_MASTER;
    attr.eSoundmode = channels == 2 ? E_MI_AUDIO_SOUND_MODE_STEREO : E_MI_AUDIO_SOUND_MODE_MONO;
    attr.u32FrmNum = 6;
    attr.u32PtNumPerFrm = frame_samples;
    attr.u32ChnCnt = channels;
    if ((r = MI_AO_SetPubAttr(0, &attr)) != MI_SUCCESS) return -2;
    if ((r = MI_AO_Enable(0)) != MI_SUCCESS) return -3;
    if ((r = MI_AO_EnableChn(0, 0)) != MI_SUCCESS) return -4;
    opened = 1;
    return 0;
}

/* Blocks until the hardware accepts the frame. `bytes` covers all channels. */
int ao_write(const void *data, unsigned bytes)
{
    MI_AUDIO_Frame_t frame;
    int tries;
    if (!opened) return -1;
    memset(&frame, 0, sizeof frame);
    frame.eBitwidth = attr.eBitwidth;
    frame.eSoundmode = attr.eSoundmode;
    frame.apVirAddr[0] = (void *)data;
    frame.u32Len = bytes;
    for (tries = 0; tries < 400; tries++) {
        if (MI_AO_SendFrame(0, 0, &frame, -1) == MI_SUCCESS) return 0;
        usleep(5000); /* buffer full: wait a little */
    }
    return -2;
}

void ao_clear(void)
{
    if (opened) MI_AO_ClearChnBuf(0, 0);
}

void ao_close(void)
{
    if (!opened) return;
    MI_AO_DisableChn(0, 0);
    MI_AO_Disable(0);
    MI_SYS_Exit();
    opened = 0;
}
