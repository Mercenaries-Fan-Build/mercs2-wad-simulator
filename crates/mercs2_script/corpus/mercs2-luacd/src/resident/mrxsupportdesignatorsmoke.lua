inherit("MrxSupportDesignator")
NETEVENT_SMOKEACTIVATE = 0
tColorList = {
  red = "0x02f6773f",
  green = "0x41675d0b",
  blue = "0x6efe9d26",
  yellow = "0xf8171566"
}
tDenialColorList = {
  red = "global_particle_flaresmoke_fail",
  green = "global_particle_flaresmoke_green_fail",
  blue = "global_particle_flaresmoke_lightblue_fail",
  yellow = "global_particle_flaresmoke_yellow_fail"
}
tColorHashToName = {}
tColorHashToName["0x02f6773f"] = "global_particle_flaresmoke"
tColorHashToName["0x41675d0b"] = "global_particle_flaresmoke_green"
tColorHashToName["0x6efe9d26"] = "global_particle_flaresmoke_lightblue"
tColorHashToName["0xf8171566"] = "global_particle_flaresmoke_yellow"
tSmokeGuids = {}
nSmokeGuids = 0

function Init()
  Pg.LoadAsset("global_weapon_m34wp", "model")
end

function Deinit()
  Pg.UnloadAsset("global_weapon_m34wp", "model")
end

function Create(self, oNewDesignator)
  oNewDesignator = oNewDesignator or {}
  oNewDesignator.uOwner = self.uOwner
  oNewDesignator.bDesignateOnDeath = false
  oNewDesignator.bDesignationComplete = self.bDesignationComplete
  oNewDesignator.sDesignationType = "Smoke Designator"
  oNewDesignator.fValidationFunction = MrxSupportDesignator.ValidateGroundDropZone
  oNewDesignator.tCallbackList = {}
  oNewDesignator.sSmokeHash = self.sSmokeHash or "0x02f6773f"
  oNewDesignator.sDenialSmokeTemplate = self.sDenialSmokeTemplate or "global_particle_flaresmoke_fail"
  oNewDesignator.sAATestLevel = "basic"
  oNewDesignator.nX = self.nX
  oNewDesignator.nY = self.nY
  oNewDesignator.nZ = self.nZ
  oNewDesignator.uGuid = self.uGuid
  setmetatable(oNewDesignator, self)
  self.__index = self
  oNewDesignator:AddCompleteCallback(DesignationCompleteCallback, {oNewDesignator})
  return oNewDesignator
end

function NetEventCallback(nEventType, tArgs)
  Debug.Printf("MrxSupportDesignatorSmoke: NetEventCallback: nEventType:" .. tostring(nEventType))
  if nEventType == NETEVENT_SMOKEACTIVATE then
    NetSafeDesignationCompleteCallback(tArgs[1], tArgs[2], tArgs[3], tArgs[4], tArgs[5])
  end
end

function DesignationCompleteCallback(self)
  if self.uGuid then
    local uHp = StringToGuid("0x16516bb1")
    local uTemplate = StringToGuid(self.sSmokeHash)
    ObjectState.StartEmitter(self.uGuid, uHp, uTemplate)
    Object.DisablePhysics(self.uGuid)
    x, y, z = Object.GetPosition(self.uGuid)
    sTemplateName = tColorHashToName[self.sSmokeHash]
    Net.SendCustomEvent("MrxSupportDesignatorSmoke", NETEVENT_SMOKEACTIVATE, {
      sTemplateName,
      x,
      y,
      z,
      tostring(self.uGuid)
    })
    Event.Create(Event.TimerRelative, {10}, RemoveSmoke, {self})
  end
end

function NetSafeDesignationCompleteCallback(sTemplateHashName, nx, ny, nz, sBeaconId)
  local uTemplateGuid = Pg.GetGuidByName(sTemplateHashName)
  tSmokeGuids[nSmokeGuids] = Pg.Spawn(uTemplateGuid, nx, ny, nz, 0)
  Event.Create(Event.TimerRelative, {10}, NetSafeRemoveSmoke, {nSmokeGuids})
  nSmokeGuids = nSmokeGuids + 1
end

function NetSafeRemoveSmoke(sBeaconId)
  if tSmokeGuids[sBeaconId] then
    Object.Remove(tSmokeGuids[sBeaconId])
    tSmokeGuids[sBeaconId] = nil
  end
end

function RemoveSmoke(self)
  if self.uGuid then
    Object.Remove(self.uGuid)
  end
end

function OnDeny(self, uGuid)
  Object.Remove(uGuid)
  if self.sDenialSmokeTemplate then
    local nX, nY, nZ = Object.GetPosition(uGuid)
    Pg.Spawn(self.sDenialSmokeTemplate, nX, nY, nZ, 0, true, true)
  end
end

function SetSmokeColor(self, sColor)
  if sColor and self then
    self.sSmokeHash = tColorList[string.lower(sColor)] or "0x02f6773f"
    self.sDenialSmokeTemplate = tDenialColorList[string.lower(sColor)] or "global_particle_flaresmoke_fail"
  end
end

function GetType(self)
  return "smoke"
end
