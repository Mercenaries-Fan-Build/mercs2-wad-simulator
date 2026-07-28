inherit("VehicleBlippable")
tFlash = {
  255,
  255,
  255
}
sTexture = "temp_radar_icon_tank"
nSize = 5

function OnActivate(uGuid, uRuntimeOwner, iArg)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Start, {
    uGuid,
    uRuntimeOwner,
    iArg
  })
end

function Start(uGuid, uRuntimeOwner, iArg)
  Debug.Printf("Tank Start")
  local health = Object.GetHealth(uGuid)
  if type(health) == "number" and 0 < health then
    local oPrototype = getfenv()
    local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
  end
end
