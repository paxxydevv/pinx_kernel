#include "../includes/pinx/ahci.h"
#include "stdint.h"
#include <stddef.h>
#include "../includes/pinx/mm/memory_management.h"
#include "../includes/pinx/kdebug.h"

/*
 * --disable-AHCI. The whole driver is inside this guard, so it is parsed but
 * contributes nothing to the binary: no code, no rodata, no .bss. The empty
 * translation unit is deliberate, and the typedef below is what keeps it a
 * valid one -- ISO C before C23 requires a TU to declare something.
 *
 * Gating here rather than in the Makefile is on purpose. A source list is
 * computed from config.mk while make parses, but this file is compiled against
 * config.h, and ./configure rewrites the pair together; leaving a build that
 * has only one of them would make the list and the preprocessor disagree and
 * fail at link time. Here both sides read the same header, so they cannot.
 */
#if !CONFIG_AHCI
typedef int ahci_disabled_placeholder;
#else

#define AHCI_PCMD_ST  (1u << 0)
#define AHCI_PCMD_FRE (1u << 4)
#define AHCI_PCMD_CR  (1u << 15)

#define AHCI_GHC_AE   (1u << 31)

#define AHCI_CL_CFL_5  (5u << 0)
#define AHCI_CL_CBR    (1u << 10)

#define AHCI_PORT_MAX 32

#define AHCI_WINDOW_BASE 0xFFFFC00000000000ULL
#define AHCI_WINDOW_SIZE 0x3000ULL
#define AHCI_CL_OFF      0x0000ULL
#define AHCI_FB_OFF      0x1000ULL
#define AHCI_CT_OFF      0x2000ULL
#define AHCI_PAGE_SIZE   0x1000ULL
#define AHCI_DMA_BASE 0xFFFFC10000000000ULL
#define AHCI_CR_SPINS    1000000

static struct ahci_hba_mem *ahci_hba;

struct ahci_port {
    uint64_t clb;
    uint64_t fb;
    uint32_t is;
    uint32_t ie;
    uint32_t cmd;
    uint32_t reserved0;
    uint32_t tfd;
    uint32_t sig;
    uint32_t ssts;
    uint32_t sctl;
    uint32_t serr;
    uint32_t sact;
    uint32_t ci;
    uint32_t sntf;
    uint32_t fbs;
    uint32_t devslp;
    uint8_t  reserved1[0x70 - 0x48];
    uint8_t  vendor[0x80 - 0x70];
};

struct ahci_hba_mem {
    uint32_t cap;
    uint32_t ghc;
    uint32_t is;
    uint32_t pi;
    uint32_t vs;
    uint32_t ccc_ctl;
    uint32_t ccc_pts;
    uint32_t em_loc;
    uint32_t em_ctl;
    uint32_t cap2;
    uint32_t bohc;
    uint8_t  reserved[0xA0 - 0x2C];
    uint8_t  vendor[0x100 - 0xA0];
    struct ahci_port ports[AHCI_PORT_MAX];
};

struct ahci_cmd_header {
    uint16_t flags;
    uint16_t prdtl;
    uint32_t prdbc;
    uint32_t ctba;
    uint32_t ctbau;
    uint32_t reserved[4];
};

struct ahci_fis_reg_h2d {
    uint8_t  fis_type;
    uint8_t  flags;
    uint8_t  command;
    uint8_t  featurel;

    uint8_t  lba0;
    uint8_t  lba1;
    uint8_t  lba2;
    uint8_t  device;

    uint8_t  lba3;
    uint8_t  lba4;
    uint8_t  lba5;
    uint8_t  featureh;

    uint8_t  countl;
    uint8_t  counth;
    uint8_t  icc;
    uint8_t  control;

    uint8_t  reserved[4];
};
struct ahci_prdt_entry {
    uint32_t dba;
    uint32_t dbau;
    uint32_t reserved;
    uint32_t dbc;
};
static uint64_t ahci_map_dma(uint8_t port_num, uint64_t offset)
{
    uint64_t physical = pmm_alloc_page();
    if (physical == PMM_NO_PAGE) {
        return PMM_NO_PAGE;
    }

    vmm_map_page(
        AHCI_WINDOW_BASE + (uint64_t)port_num * AHCI_WINDOW_SIZE + offset,
        physical);

    return physical;
}

static void ahci_reset_page(uint64_t address)
{
    volatile uint32_t *words = (volatile uint32_t *)address;
    for (uint64_t i = 0; i < AHCI_PAGE_SIZE / sizeof(uint32_t); i++) {
        words[i] = 0;
    }

    volatile uint8_t *lines = (volatile uint8_t *)address;
    for (uint64_t i = 0; i < AHCI_PAGE_SIZE; i += 64) {
        __asm__ volatile ("clflush (%0)" :: "r"(lines + i) : "memory");
    }

    __asm__ volatile ("sfence" ::: "memory");
}
bool ahci_read(uint8_t port_num)
{
    uint64_t page = pmm_alloc_page();

    if (page == PMM_NO_PAGE) {
        KERROR("PMM Alloc Failed <- ahci_read()");
        return false;
    }

    uint64_t buffer_virtual = AHCI_DMA_BASE;

    vmm_map_page(buffer_virtual, page);

    ahci_reset_page(buffer_virtual);

    uint64_t ct_virtual =
        AHCI_WINDOW_BASE
        + (uint64_t)port_num * AHCI_WINDOW_SIZE
        + AHCI_CT_OFF;

    struct ahci_prdt_entry *prdt =
        (struct ahci_prdt_entry *)(ct_virtual + 0x80);

    prdt->dba = (uint32_t)(page & 0xFFFFFFFF);
    prdt->dbau = (uint32_t)(page >> 32);
    prdt->reserved = 0;
    prdt->dbc = 4095;

    return true;
}
void init_ahci(uintptr_t hba, uint8_t port_num)
{
    if (port_num >= AHCI_PORT_MAX) {
        KERROR("ahci: port %u is out of range\n", (unsigned)port_num);
        return;
    }

    struct ahci_hba_mem *hba_mem = (struct ahci_hba_mem *)hba;
    struct ahci_port *port = &hba_mem->ports[port_num];

    hba_mem->ghc |= AHCI_GHC_AE;

    port->cmd &= ~AHCI_PCMD_FRE;
    port->cmd &= ~AHCI_PCMD_ST;

    for (uint64_t spin = 0; spin < AHCI_CR_SPINS; spin++) {
        if (!(port->cmd & AHCI_PCMD_CR)) {
            break;
        }

        __asm__ volatile ("pause");
    }

    if (port->cmd & AHCI_PCMD_CR) {
        KERROR("ahci: port %u never went idle, PxCMD=0x%08X\n",
               (unsigned)port_num, (unsigned)port->cmd);
        return;
    }

    port->is = 0xFFFFFFFF;
    port->serr = 0xFFFFFFFF;

    uint64_t window =
        AHCI_WINDOW_BASE + (uint64_t)port_num * AHCI_WINDOW_SIZE;

    uint64_t cl_phys = ahci_map_dma(port_num, AHCI_CL_OFF);
    uint64_t fb_phys = ahci_map_dma(port_num, AHCI_FB_OFF);
    uint64_t ct_phys = ahci_map_dma(port_num, AHCI_CT_OFF);

    if (cl_phys == PMM_NO_PAGE ||
        fb_phys == PMM_NO_PAGE ||
        ct_phys == PMM_NO_PAGE) {
        KERROR("ahci: port %u is out of memory\n", (unsigned)port_num);
        return;
    }

    ahci_reset_page(window + AHCI_CL_OFF);
    ahci_reset_page(window + AHCI_FB_OFF);
    ahci_reset_page(window + AHCI_CT_OFF);

    volatile struct ahci_cmd_header *header =
        (volatile struct ahci_cmd_header *)(window + AHCI_CL_OFF);

    header->flags = (uint16_t)(AHCI_CL_CFL_5 | AHCI_CL_CBR);
    header->prdtl = 0;
    header->prdbc = 0;
    header->ctba = (uint32_t)(ct_phys & 0xFFFFFFFF);
    header->ctbau = (uint32_t)(ct_phys >> 32);

    port->clb = cl_phys;
    port->fb = fb_phys;

    __asm__ volatile ("mfence" ::: "memory");

    port->cmd |= AHCI_PCMD_FRE;

    KINFO("ahci: port %u up, PxCLB=0x%lX PxFB=0x%lX CT=0x%lX\n",
          (unsigned)port_num,
          (unsigned long)cl_phys,
          (unsigned long)fb_phys,
          (unsigned long)ct_phys);
}

_Static_assert(offsetof(struct ahci_hba_mem, ports) == 0x100,
               "AHCI HBA ports offset is wrong");

_Static_assert(sizeof(struct ahci_port) == 0x80,
               "AHCI port register block must be 128 bytes");

_Static_assert(sizeof(struct ahci_cmd_header) == 32,
               "AHCI command header must be 32 bytes");

_Static_assert(sizeof(struct ahci_fis_reg_h2d) == 20,
               "AHCI Register H2D FIS must be 20 bytes");

#endif /* CONFIG_AHCI */