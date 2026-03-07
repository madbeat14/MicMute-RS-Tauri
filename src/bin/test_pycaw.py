import pycaw.pycaw as caw
import comtypes

enumerator = caw.AudioUtilities.GetDeviceEnumerator()
devices = caw.AudioUtilities.GetAllDevices()

print("ALL DEVICES IN PYCAW:")
for d in devices:
    state = d.state
    # 1=Active, 2=Disabled, 4=NotPresent, 8=Unplugged
    if state == 1:
        print(f"[{d.id}] '{d.FriendlyName}' (state={state})")

try:
    mic = caw.AudioUtilities.GetMicrophone()
    print(f"\nDEFAULT MIC COM ID: [{mic.GetId()}]")
except Exception as e:
    print(e)
