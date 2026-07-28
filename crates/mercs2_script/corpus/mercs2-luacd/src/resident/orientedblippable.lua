inherit("Blippable")
bRotate = true

function OnActivate(uGuid, uRuntimeOwner, iArg)
  Debug.Printf("OrientedBlippable OnActivate")
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
end

function TimerCallback(oSelf)
  oSelf:AddObjective(oSelf.bFlash)
end

function SetBlipped(oSelf)
  oSelf.bOriented = true
  Blippable.SetBlipped(oSelf)
  if not oSelf.TimerEvent and oSelf.bFlash then
    oSelf.TimerEvent = Event.CreatePersistent(Event.TimerRelative, {0.05}, TimerCallback, {oSelf})
  end
end

function ClearBlipped(oSelf)
  if oSelf.TimerEvent then
    Event.Delete(oSelf.TimerEvent)
    oSelf.TimerEvent = nil
  end
  Blippable.ClearBlipped(oSelf)
end
