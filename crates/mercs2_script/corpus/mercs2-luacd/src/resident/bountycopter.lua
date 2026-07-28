tEvents = tEvents or {}
inherit("VehicleBlippable")
tFlash = {
  255,
  255,
  255
}
sTexture = "temp_radar_icon_helicopter"
nSize = 5

function OnActivate(uGuid, uRuntimeOwner, iArg)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Start, {
    uGuid,
    uRuntimeOwner,
    iArg
  })
end

function Start(uGuid, uRuntimeOwner, iArg)
  local x, y, z = Object.GetPosition(uGuid)
  local uCargo = ""
  if Object.HasLabel(uGuid, "Blueprints") then
    Debug.Printf("Creating blueprints")
    uCargo = Pg.Spawn("Supply Drop (Blueprints)", x, y + 200, z)
  elseif Object.HasLabel(uGuid, "Treasure") then
    uCargo = Pg.Spawn("Supply Drop (Treasure)", x, y + 200, z)
    Debug.Printf("Creating treasure")
  else
    uCargo = Pg.Spawn("Supply Drop (Light MG)", x, y + 200, z)
    Debug.Printf("Creating supply drop")
  end
  Event.Create(Event.ObjectHibernation, {uCargo, "awake"}, _DeployWinch, {uGuid, uCargo})
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
end

function _DeployWinch(uGuid, uCargo)
  Object.SetWinchState(uGuid, "deployed")
  Event.Create(Event.TimerRelative, {0.1}, AttachCargo, {uGuid, uCargo})
end

function AttachCargo(uGuid, uCargo)
  Object.AttachCargoToWinch(uCargo, uGuid)
end
