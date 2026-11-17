/* Audio output for the Miyoo Mini, in C so the calls are plain libc ones.
 *
 * 1. OSS /dev/dsp. Under MinUI this is the one to use: MinUI starts the
 *    firmware's audioserver (which owns the hardware) and preloads
 *    libpadsp.so into every app; that library intercepts open/ioctl/write on
 *    /dev/dsp and forwards the audio to the server.
 * 2. The SigmaStar MI_AO API directly (device 0 / channel 0, 16-bit PCM), for
 *    when no audio server is running. */
#include <errno.h>
#include <fcntl.h>
#include <string.h>
#include <unistd.h>
#include <sys/ioctl.h>
#include <sys/soundcard.h>
#include <mi_sys.h>
#include <mi_ao.h>

static int dsp_fd = -1;

/* Returns the rate actually set (> 0), or -step on failure.
 *
 * Workarounds for the firmware's libpadsp.so (checked on a Mini+):
 * - its open() hook only looks up the real open() lazily on the non-audio
 *   path, but the /dev/dsp path calls it unchecked, so if /dev/dsp is the first
 *   file opened through open() it jumps to NULL. Rust's own file I/O uses
 *   open64(), which isn't hooked, so open something harmless first.
 * - SNDCTL_DSP_RESET crashes it too (see dsp_reset). */
int dsp_open(unsigned rate, unsigned channels)
{
    int fd, fmt = AFMT_S16_LE, ch = (int)channels, speed = (int)rate;
    int warm = open("/dev/null", O_RDONLY);
    if (warm >= 0) close(warm);
    if ((fd = open("/dev/dsp", O_WRONLY)) < 0) return -1;
    if (ioctl(fd, SNDCTL_DSP_SETFMT, &fmt) < 0 || fmt != AFMT_S16_LE) { close(fd); return -2; }
    if (ioctl(fd, SNDCTL_DSP_CHANNELS, &ch) < 0 || ch != (int)channels) { close(fd); return -3; }
    if (ioctl(fd, SNDCTL_DSP_SPEED, &speed) < 0 || speed <= 0) { close(fd); return -4; }
    dsp_fd = fd;
    return speed;
}

int dsp_write(const void *data, unsigned bytes)
{
    const char *p = data;
    while (bytes > 0) {
        ssize_t n = write(dsp_fd, p, bytes);
        if (n < 0) {
            if (errno == EINTR) continue;
            return -1;
        }
        p += n;
        bytes -= (unsigned)n;
    }
    return 0;
}

/* Deliberately does nothing: SNDCTL_DSP_RESET segfaults inside libpadsp.so.
 * Only a few tens of ms are buffered (4 KB pipe + the server), so after a
 * seek that little bit of old audio just plays out. */
void dsp_reset(void)
{
}

void dsp_close(void)
{
    if (dsp_fd >= 0) close(dsp_fd);
    dsp_fd = -1;
}

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
