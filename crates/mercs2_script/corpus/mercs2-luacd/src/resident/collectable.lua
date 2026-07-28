inherit("Inheritable")
import("MrxGui")
import("MrxPmc")

function OnActivate(uGuid, uRuntimeOwner, iArg)
  if not Object.IsAlive(uGuid) then
    return
  end
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
end

function Create(oPrototype, uGuid, uRuntimeOwner)
  local oInstance = Inheritable.Create(oPrototype, uGuid, uRuntimeOwner)
  if not Object.IsAlive(uGuid) then
    return oInstance
  end
  if Object.HasLabel(uGuid, "CollectableInvalidated") then
    Object.SetHibernationDistance(uGuid, 1.0E-6)
    Object.Kill(uGuid)
    return oInstance
  end
  Pg.AddContextAction(uGuid, "[ContextAction.Toolbox]", 2, 0, 0, 0, 0)
  oInstance.uEvent = Event.CreatePersistent(Event.ContextAction, {
    Player.GetAnyCharacter(),
    uGuid
  }, oInstance.OnContextAction, {oInstance})
  return oInstance
end

function Delete(oSelf)
  Event.Delete(oSelf.uEvent)
  Pg.RemoveContextAction(oSelf.uGuid)
  Inheritable.Delete(oSelf)
end

function OnContextAction(oSelf, uCharacter)
  Object.Kill(oSelf.uGuid)
end
