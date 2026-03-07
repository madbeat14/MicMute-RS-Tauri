import pycaw.pycaw as caw
from comtypes import CLSCTX_ALL
import time
import math

device_id = "{0.0.1.00000000}.{b6eff619-2ed0-4ec1-9c18-bbc33a802301}"

devices = caw.AudioUtilities.GetAllDevices()
found = [d for d in devices if d.id == device_id]
if not found:
    print("Device not found")
else:
    dev = found[0]
    meter = dev.Activate(
        caw.IAudioMeterInformation._iid_, CLSCTX_ALL, None)
    
    print(f"Reading VU meter for {dev.FriendlyName} ({dev.id})...")
    for _ in range(10):
        time.sleep(0.5)
        print(f"Peak: {meter.GetPeakValue()}")
