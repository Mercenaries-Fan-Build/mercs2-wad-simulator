inherit("VehicleBlippable")
tFlash = {
  255,
  255,
  255
}
sTexture = "temp_radar_icon_airplane"
nSize = 5

function OnActivate(uGuid, uRuntimeOwner, iArg)
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
end
