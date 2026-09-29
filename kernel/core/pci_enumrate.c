#include <stdint.h>
#include "pinx/io.h"
#include "pinx/pci_enumrate.h"
#include "pinx/kdebug.h"
#include "pinx/mm/memory_management.h"
#include "pinx/terminal.h"
#if CONFIG_AHCI
#include "pinx/ahci.h"
#define ABAR_MMIO 0xffffb00000000000ULL
#endif

uint32_t pci_config_read32(
    uint8_t bus,
    uint8_t slot,
    uint8_t func,
    uint8_t offset
) {
    uint32_t address =
        (1U << 31) |
        ((uint32_t)bus  << 16) |
        ((uint32_t)slot << 11) |
        ((uint32_t)func << 8) |
        (offset & 0xFC);

    outl(0xCF8, address);
    return inl(0xCFC);
}

bool dev_check(uint32_t id) {
    uint16_t vendor = id & 0xFFFF;
    return vendor != 0xFFFF;
}

void pci_enumerate(void)
{
    for (uint16_t bus = 0; bus < 256; bus++) {
        for (uint8_t slot = 0; slot < 32; slot++) {
            for (uint8_t func = 0; func < 8; func++) {

                uint32_t id = pci_config_read32(
                    (uint8_t)bus,
                    slot,
                    func,
                    0x00
                );

                if (!dev_check(id))
                    continue;

                uint32_t class_info = pci_config_read32(
                    (uint8_t)bus,
                    slot,
                    func,
                    0x08
                );

                uint8_t revision = class_info & 0xFF;
                uint8_t prog_if   = (class_info >> 8) & 0xFF;
                uint8_t subclass  = (class_info >> 16) & 0xFF;
                uint8_t class     = (class_info >> 24) & 0xFF;

                KPCI(
                    "PCI %02X:%02X.%X class=%02X subclass=%02X "
                    "prog-if=%02X rev=%02X\n",
                    (uint8_t)bus,
                    slot,
                    func,
                    class,
                    subclass,
                    prog_if,
                    revision
                );

                /*
                 * Mass storage controller
                 * SATA controller
                 * AHCI 1.0
                 */
                if (class != 0x01 ||
                    subclass != 0x06 ||
                    prog_if != 0x01) {
                    continue;
                }

#if CONFIG_AHCI
                /*
                 * BAR5 contains the AHCI ABAR.
                 *
                 * For a 32-bit MMIO BAR, the low four bits are flags,
                 * so remove them before using the physical address.
                 */
                uint32_t bar5 = pci_config_read32(
                    (uint8_t)bus,
                    slot,
                    func,
                    0x24
                );

                uint32_t abar = bar5 & ~0xFULL;

                /*
                 * The register file is 0x2000 bytes, not 0x1000: the HBA
                 * occupies the first page and the 32 port blocks run from
                 * 0x100 to 0x2000. Mapping only the first page leaves ports
                 * 16-31 unmapped, which is a page fault waiting for a
                 * controller that implements that many.
                 */
                vmm_map_mmio(ABAR_MMIO, abar);
                vmm_map_mmio(ABAR_MMIO + 0x1000, abar + 0x1000);

                KPCI(
                    "AHCI BAR5 = 0x%08X\n",
                    abar
                );

                /*
                 * HBA registers
                 */

                uint32_t cap =
                    *(volatile uint32_t *)(ABAR_MMIO + 0x00);

                uint32_t np = cap & 0x1F;
                uint32_t port_count = np + 1;

                KPCI(
                    "AHCI CAP = 0x%08X\n",
                    cap
                );

                KPCI(
                    "AHCI ports supported = %u\n",
                    port_count
                );

                /*
                 * PI = Ports Implemented
                 */
                uint32_t pi =
                    *(volatile uint32_t *)(ABAR_MMIO + 0x0C);

                KPCI(
                    "AHCI PI = 0x%08X\n",
                    pi
                );

                /*
                 * Check every port supported by CAP.NP.
                 */
                for (uint8_t port = 0;
                     (uint32_t)port < port_count;
                     port++) {

                    /*
                     * PI bit tells us whether this port is implemented.
                     */
                    if (!(pi & (1U << port)))
                        continue;

                    /*
                     * Each AHCI port register block is 0x80 bytes.
                     *
                     * Port 0 = ABAR + 0x100
                     * Port 1 = ABAR + 0x180
                     * Port 2 = ABAR + 0x200
                     * ...
                     */
                    uint64_t port_base =
                        ABAR_MMIO +
                        0x100 +
                        ((uint64_t)port * 0x80);

                    /*
                     * PxSSTS = port + 0x28
                     *
                     * DET = bits 0-3
                     * IPM = bits 8-11
                     */
                    uint32_t ssts =
                        *(volatile uint32_t *)(port_base + 0x28);

                    uint32_t det =
                        ssts & 0xF;

                    uint32_t ipm =
                        (ssts >> 8) & 0xF;

                    /*
                     * PxSIG = port + 0x24
                     */
                    uint32_t pxsig =
                        *(volatile uint32_t *)(port_base + 0x24);

                    KPCI(
                        "port %u: SSTS=0x%08X "
                        "DET=%u IPM=%u "
                        "SIG=0x%08X\n",
                        port,
                        ssts,
                        det,
                        ipm,
                        pxsig
                    );

                    /*
                     * DET = 3:
                     * Device present and communication established.
                     *
                     * IPM = 1:
                     * Interface is in the active power-management state.
                     */
                    if (det == 3 && ipm == 1) {

                        /*
                         * 0x00000101 = ATA/SATA device
                         * 0xEB140101 = ATAPI device
                         */
                        if (pxsig == 0x00000101) {
                            KPCI(
                                "SATA ATA device detected "
                                "on port %u\n",
                                port
                            );
                            /* ABAR_MMIO, not abar: abar is physical. */
                            init_ahci(ABAR_MMIO, port);
                        }
                        else if (pxsig == 0xEB140101) {
                            KPCI(
                                "SATA ATAPI device detected "
                                "on port %u\n",
                                port
                            );
                        }
                        else {
                            KPCI(
                                "Unknown SATA device signature "
                                "on port %u: 0x%08X\n",
                                port,
                                pxsig
                            );
                        }
                    }
                }
#else
                /*
                 * --disable-AHCI. The controller was found and is already
                 * listed by the DEBUG_PCI line above, but its ABAR is never
                 * mapped and no port is programmed, so init_ahci() and
                 * kernel/drivers/ahci.c are not in the build.
                 *
                 * Said out loud rather than dropped silently: a boot log with
                 * no AHCI lines at all is indistinguishable from a machine
                 * that has no SATA controller, and that is the wrong thing to
                 * have to guess about while debugging the enumeration.
                 */
                KPCI(
                    "AHCI controller at %02X:%02X.%X detected, "
                    "setup compiled out (--disable-AHCI)\n",
                    (uint8_t)bus,
                    slot,
                    func
                );
#endif
            }
        }
    }
}
