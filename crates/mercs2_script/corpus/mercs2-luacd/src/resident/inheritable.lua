tInstance = {}

function OnActivate(uGuid, uRuntimeOwner, iArg)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Awake, {uGuid, iArg})
end

function Awake(uGuid, iArg)
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, iArg)
end

function OnDeactivate(uGuid)
  local oInstance = tInstance[uGuid]
  if oInstance then
    oInstance:Delete()
  end
end

function OnDeath(uGuid)
  OnDeactivate(uGuid)
end

function Create(oPrototype, uGuid, iArg)
  local oInstance = {}
  setmetatable(oInstance, {__index = oPrototype})
  oInstance.uGuid = uGuid
  oInstance.sName = tostring(uGuid)
  tInstance[uGuid] = oInstance
  return oInstance
end

function Delete(oSelf)
  tInstance[oSelf.uGuid] = nil
end

function GetFromGuid(uGuid)
  return tInstance[uGuid]
end
