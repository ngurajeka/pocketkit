Wifi.pak - Wi-Fi control for MinUI on the Miyoo Mini Plus

SETUP
1. Copy this folder to  Tools/miyoomini/Wifi.pak  on the SD card.
2. Driver: install ADBD.pak from the MinUI Extras (it contains 8188fu.ko),
   or copy 8188fu.ko into this folder.
3. Copy networks.example.txt to networks.txt, put your Wi-Fi name and
   password in it.

USE
Tools > Wifi
- First run imports networks.txt, turns Wi-Fi on and shows your IP address.
- Next run turns Wi-Fi off.
Everything it does is written to log.txt in this folder.

Your previous Wi-Fi settings are saved to backup/ before networks.txt replaces them.
Needs a MinUI release that includes say.elf (October 2025 or newer).
