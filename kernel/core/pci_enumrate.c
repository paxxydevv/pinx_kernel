#include <stdint.h>
#include "pinx/io.h"
#include "pinx/pci_enumrate.h"
#include "pinx/kdebug.h"
uint32_t pci_config_read32(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset) {
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

                KPCI("PCI device: 0x%08X\n", id);
            }
        }
    }
}