inherit("Blippable")
tColor = {
  255,
  0,
  0
}
tFlash = {
  255,
  255,
  255
}
sTexture = nil
nSize = 1
nSortOrder = 1

function OnActivate(uGuid, uRuntimeOwner, iArg)
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
end

function SetBlipped(oSelf)
  Blippable.SetBlipped(oSelf)
  if not oSelf.TimerEvent then
    oSelf.TimerEvent = Event.CreatePersistent(Event.TimerRelative, {0.1}, function(oSelf)
      oSelf.bFlash = not oSelf.bFlash
      oSelf:AddObjective(oSelf.bFlash)
    end, {oSelf})
  end
end

function ClearBlipped(oSelf)
  if oSelf.TimerEvent then
    Event.Delete(oSelf.TimerEvent)
    oSelf.TimerEvent = nil
  end
  Blippable.ClearBlipped(oSelf)
end

function _HomingLaunched(oWidget, tData)
  local oInstance = GetFromGuid(tData.uAmmoGuid)
  if not oInstance then
    return
  end
  oInstance.bActive = true
  oInstance:SetBlipped()
end
