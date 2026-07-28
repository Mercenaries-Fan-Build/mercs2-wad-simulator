inherit("MrxTaskObjective")
import("MrxUtil")
import("MrxSupportData")
import("MrxTutorialManager")
import("MrxVerifyManager")
import("MrxVoSequence")
import("MrxFactionManager")
import("MrxSupportPickup")
NETEVENT_VERIFY = 0
local HVTNORMAL = 1
local HVTSUBDUED = 2
local HELIDESTROYED = 3
local HELIDAMAGED = 4
local HELILANDED = 5
local HELIPILOTKILLED = 6
local HVTDEAD = 7
local tHasExtractionFreebie = {}
local tHVTDistanceEvents = {}

function Activated(self)
  self.nCurrHVTState = HVTNORMAL
  MrxTaskObjective.Activated(self)
  self._nSupportCount = self._nSupportCount or 0
  _evClientJoined = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, SendPlayerJoinEvents)
  self._tEvents.uDeathEvent = Event.CreatePersistent(Event.ObjectDeath, {
    self._uTgtObjFilter
  }, self._TargetDestroyed, {self})
  self._tEvents.uProxEvent = {}
  for target, data in pairs(self._tTargets) do
    self._tEvents.uProxEvent[target] = Event.Create(Event.ObjectProximity, {
      "hero",
      target,
      "<",
      10,
      false,
      false
    }, self.HeroProximity, {self, target})
  end
  self:_CreatePersistentEvent(Event.HumanStateTransition, {
    self._uTgtObjFilter,
    "*",
    "KnockedDown.Idle"
  }, self._TargetBashed, {self})
  self:_CreatePersistentEvent(Event.HumanStateTransition, {
    self._uTgtObjFilter,
    "KnockedDown.*",
    "Upright.*"
  }, self._TargetOutOfSubdued, {self})
  local tConfig = self:GetConfig()
  local oSupport = MrxSupportData.GetFreebie(_GetSupportByFaction(tConfig.sFactionId))
  if oSupport then
    oSupport.oSupport:SetHeliDestroyedCB(_HelicopterDestroyedCallback, self)
    oSupport.oSupport:SetHeliLandedCB(_HelicopterLandedCallback, self)
    oSupport.oSupport:SetPilotKilledCB(_PilotKilledCallback, self)
    oSupport.oSupport:SetHeliSpawnedCB(_HelicopterSpawnedCallback)
    oSupport.oSupport:SetHeliDamagedCB(_HelicopterDamagedCallback, self)
  end
  self._bIsVerified = false
  local tTgtInclude = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  bActivating = true
  for _, uGuid in ipairs(tTgtInclude) do
    _TargetOnHibernate(self, uGuid)
    MrxVerifyManager.AddTarget(uGuid)
  end
  bActivating = false
end

function Cleanup(self)
  self:_TargetOutOfSubdued()
  for uGuid, distanceEvent in pairs(tHVTDistanceEvents) do
    self:_RemoveSupport(uGuid)
  end
  MrxTaskObjective.Cleanup(self)
end

tActiveHelicopters = {}

function _HelicopterSpawnedCallback(oSupport, uHeli)
  tActiveHelicopters[uHeli] = oSupport
  Event.Create(Event.ObjectDelete, {uGuid}, function(uHeli)
    tActiveHelicopters[uHeli] = nil
  end, {uHeli})
end

function _TargetIntoSubdued(self)
  if not self._bIsVerified then
    if self.nCurrHVTState ~= HELIDESTROYED and self.nCurrHVTState ~= HELIDAMAGED and self.nCurrHVTState ~= HELIPILOTKILLED and self.nCurrHVTState ~= HELILANDED then
      self.nCurrHVTState = HVTSUBDUED
    end
    if not Sys.IsGermanSKU() then
      MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key1]")
    end
  end
end

function _HelicopterDestroyedCallback(self)
  if not self._bIsVerified then
    self.nCurrHVTState = HELIDESTROYED
    MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key2]")
  end
end

function _HelicopterDamagedCallback(self)
  if not self._bIsVerified then
    self.nCurrHVTState = HELIDAMAGED
    MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key1]")
  end
end

function _PilotKilledCallback(self)
  self.nCurrHVTState = HELIPILOTKILLED
  MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key1]")
end

function _HelicopterLandedCallback(self)
  self.nCurrHVTState = HELILANDED
  MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key3]")
end

function _TargetOutOfSubdued(self)
  if not self._bIsVerified and not self._bIsDead then
    self.nCurrHVTState = HVTNORMAL
    MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key4]")
  end
end

function HeroProximity(self, uGuid, uHero)
  Debug.Printf("HeroProximity: " .. tostring(uGuid))
  local sFaction = MrxUtil.GetFaction(uGuid)
  local uFaction = Pg.GetGuidByName(sFaction)
  local VO = {}
  if Object.HasLabel(uGuid, "Coward") then
    VO = {
      Guerilla = {
        "VZCivMale-In-Mission-Contract-Pmc01-50",
        "VZCivMale-In-Mission-Contract-Pmc01-55",
        "VZCivMale-In-Mission-Contract-Pmc01-58",
        "VZSoldier-In-Mission-Contract-Pmc01-57"
      },
      VZ = {
        "VZCivMale-In-Mission-Contract-Pmc01-50",
        "VZCivMale-In-Mission-Contract-Pmc01-55",
        "VZCivMale-In-Mission-Contract-Pmc01-58",
        "VZSoldier-In-Mission-Contract-Pmc01-57"
      }
    }
    Ai.SetState({
      AIGuid = uGuid,
      State = "Pacifist",
      Value = true
    })
    Human.DoAction(uGuid, "Cower")
    Ai.Goal({
      AIGuid = uGuid,
      Goal = "Idle",
      Priority = "HiPri",
      Force = true
    })
  else
    VO = {
      Allied = {
        "AlliedSoldier01.Subdued.Taunt01",
        "AlliedSoldier01.Subdued.Taunt03"
      },
      China = {
        "ChinaSoldier01.Subdued.Taunt01",
        "ChinaSoldier01.Subdued.Taunt02",
        "ChinaSoldier01.Subdued.Taunt03"
      },
      Guerilla = {
        "GurSoldier01.Subdued.Taunt01",
        "GurSoldier01.Subdued.Taunt02",
        "GurSoldier01.Subdued.Taunt03"
      },
      OC = {
        "OCSoldier01.Subdued.Taunt01"
      },
      VZ = {
        "VZSoldier01.Subdued.Taunt01",
        "VZSoldier01.Subdued.Taunt02",
        "VZSoldier01.Subdued.Taunt03"
      }
    }
  end
  if VO[sFaction] and not Object.HasLabel("Blanco") then
    local sCue = MrxUtil.GetRandomTableElement(VO[sFaction])
    MrxVoSequence.Start(sCue, uGuid, MrxVoSequence.knPriorityFreeplay)
  end
  if uFaction then
    Ai.AddInfraction(Player.GetPrimaryCharacter(), uFaction, 5)
    if Player.GetSecondaryCharacter() then
      Ai.AddInfraction(Player.GetSecondaryCharacter(), uFaction, 5)
    end
  else
    Debug.Printf("No faction found???")
  end
end

function _TargetOnHibernate(self, uGuid)
  if not bActivating then
    MrxTutorialManager.HideMessage()
  end
  self._tEvents.uOnAwakeEvent = self._tEvents.uOnAwakeEvent or {}
  self._tEvents.uOnAwakeEvent[uGuid] = Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, self._TargetOnAwake, {self, uGuid})
end

function _TargetOnAwake(self, uGuid)
  if self._bIsVerified or self.nCurrHVTState == nil then
  elseif self.nCurrHVTState == HVTNORMAL then
    self:_TargetOutOfSubdued(self)
  elseif self.nCurrHVTState == HVTSUBDUED then
    self:_TargetIntoSubdued(self)
  elseif self.nCurrHVTState == HELIDESTROYED then
    self:_HelicopterDestroyedCallback()
  elseif self.nCurrHVTState == HELIDAMAGED or self.nCurrHVTState == HELIPILOTKILLED then
    self:_HelicopterDamagedCallback(self)
  elseif self.nCurrHVTState == HELILANDED then
    _HelicopterLandedCallback(self)
  elseif self.nCurrHVTState == HVTDEAD then
    MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key5]")
  end
  Debug.Printf("CorpseCleanup: " .. tostring(Human.SetAllowCorpseCleanup(uGuid, false)))
  self._tEvents.uOnHibernateEvent = self._tEvents.uOnHibernateEvent or {}
  self._tEvents.uOnHibernateEvent[uGuid] = Event.Create(Event.ObjectHibernation, {uGuid, "hibernated"}, self._TargetOnHibernate, {self, uGuid})
end

function _TargetDestroyed(self, uGuid)
  self._bIsDead = true
  local tConfig = self:GetConfig()
  MrxUtil.ProcessCallbackTable(tConfig.tOnTargetDestroyed, {uGuid})
  MrxUtil.CallWithOptionalArgs(tConfig.fOnTargetDestroyed, {uGuid})
  if self._tEvents.uProxEvent and self._tEvents.uProxEvent[uGuid] then
    Event.Delete(self._tEvents.uProxEvent[uGuid])
    self._tEvents.uProxEvent[uGuid] = nil
  end
  Pg.RemoveContextAction(uGuid)
  if Sys.IsGermanSKU() then
    Debug.Printf("CENSORED!!!!")
    _TargetActionedComplete(self, uGuid)
  else
    Event.Create(Event.TimerRelative, {3}, self._SetupVerify, {self, uGuid})
    self:_RemoveSupport(uGuid)
  end
end

function _SetupVerify(self, uGuid)
  self.nCurrHVTState = HVTDEAD
  MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key5]")
  self._tEvents.uActionEvent = self._tEvents.uActionEvent or {}
  Pg.AddContextAction(uGuid, "[ContextAction.Verify]", 5, 0, 0, 0, 0)
  self._tEvents.uActionEvent[uGuid] = Event.Create(Event.ContextAction, {
    Player.GetAnyCharacter(),
    uGuid
  }, self._TargetActioned, {self})
  ASSERT(self._tEvents.uActionEvent[uGuid])
end

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_VERIFY then
    NetSafeTargetActioned(tArgs[1], tArgs[2])
  end
end

function _TargetActioned(self, uActioner, uGuid)
  Net.SendCustomEvent("MrxTaskObjectiveVerify", NETEVENT_VERIFY, {uActioner, uGuid})
  NonNetSafeTargetActioned(self, uActioner, uGuid)
end

function DoVerifyAnimation(uActioner, uGuid)
  local tX, tY, tZ = Object.GetPosition(uGuid)
  local pX, pY, pZ = Object.GetPosition(uActioner)
  local nYaw = Math.GetXZHeading(tX - pX, tY - pY, tZ - pZ)
  Object.SetYaw(uActioner, nYaw)
  local uPlayerCamera = Player.GetCamera(Object.IsPlayerControlled(uActioner))
  if uPlayerCamera then
    Camera.SetYaw(uPlayerCamera, 0)
  end
  local uCamera = Pg.Spawn("Verification Camera", Object.GetPosition(uActioner))
  Object.Attach(uActioner, "bone_attach_lhand", uCamera)
  Player.SetInputEnabled(Object.IsPlayerControlled(uActioner), false, true)
  Human.DoAction(uActioner, "VerifyCamera")
  FlashEvent = Event.Create(Event.TimerRelative, {2}, flashAnimation)
  return uCamera
end

function Abort(self, uActioner, FlashEvent)
  Event.Delete(FlashEvent)
end

function flashAnimation()
  Sound.CueSound(0, "ui_camera")
  Pg.SpawnFromCamera("verify flash", 0, 0)
end

function NetSafeTargetActioned(uActioner, uGuid)
  local uCamera = DoVerifyAnimation(uActioner, uGuid)
  Event.Create(Event.HumanActionComplete, {uActioner}, NetSafeTargetActionedComplete, {
    uActioner,
    uCamera,
    Object.GetHealth(uActioner)
  })
end

function NonNetSafeTargetActioned(self, uActioner, uGuid)
  local uCamera = DoVerifyAnimation(uActioner, uGuid)
  Pg.RemoveContextAction(uGuid)
  Event.Create(Event.HumanActionComplete, {uActioner}, self._TargetActionedComplete, {
    self,
    uGuid,
    uActioner,
    uCamera,
    Object.GetHealth(uActioner)
  })
end

function _TargetActionedComplete(self, uGuid, uActioner, uCamera, nOldHealth)
  MrxVerifyManager.UpdateTarget(uGuid, "killed")
  self.nCurrHVTState = nil
  MrxTutorialManager.HideMessage()
  self._bIsVerified = true
  local tConfig = self:GetConfig()
  MrxUtil.ProcessCallbackTable(tConfig.tOnTargetActioned, {uGuid})
  MrxUtil.CallWithOptionalArgs(tConfig.fOnTargetActioned, {uGuid})
  Human.SetAllowCorpseCleanup(uGuid, true)
  if uCamera then
    Object.Remove(uCamera)
    Object.EnablePhysics(uActioner)
    Player.SetInputEnabled(Object.IsPlayerControlled(uActioner), true, true)
  end
  if FlashEvent then
    Event.Delete(FlashEvent)
  end
  if type(uGuid) == "userdata" then
    self:RemoveTarget(uGuid)
  end
  self:CompletePart(uGuid, true)
end

function NetSafeTargetActionedComplete(uActioner, uCamera, nOldHealth)
  if uCamera then
    Object.Remove(uCamera)
    Player.SetInputEnabled(Object.IsPlayerControlled(uActioner), true, true)
  end
  if FlashEvent then
    Event.Delete(FlashEvent)
  end
end

function _TargetBashed(self, uGuid)
  self.nCurrHVTState = HVTNORMAL
  MrxTutorialManager.ShowMessage("[Tutorial.ObjectiveVerify.Key6]")
  Pg.AddContextAction(uGuid, "[ContextAction.Subdue]", 2)
  self._uSubdueEvent = self:_CreatePersistentEvent(Event.HumanStateTransition, {
    uGuid,
    "*",
    "Subdued.Idle"
  }, self._TargetSubdued, {self})
end

_tSubduedVO = {
  Allied = {
    "Allied NY_Richard01_Soldier_AI Subdual_x_x_x_x_x_01",
    "Mirron01_Soldier_AI Subdual_x_x_x_x_x_01",
    "Matt01_Soldier_AI Subdual_x_x_x_x_x_01"
  },
  China = {
    "China Soldier_Ming01_Soldier_AI Subdual_x_x_x_x_x_01"
  },
  Guerilla = {
    "VZSoldierArc_Zev01_Soldier_AI Subdual_x_x_x_x_x_01",
    "VZSoldierArc_Zev01_VZ Soldier_AI Subdual_x_x_x_x_x_08"
  },
  OC = {
    "Generic OC Soldier_Keith01_Soldier_AI Subdual_x_x_x_x_x_01",
    "Generic OC Soldier_Derek01_Soldier_AI Subdual_x_x_x_x_x_01"
  },
  Pirate = {
    "Pirate Thug_Jonell01_Soldier_AI Subdual_x_x_x_x_x_01",
    "Pirate Thug_Darryl01_Soldier_AI Subdual_x_x_x_x_x_01"
  },
  VZ = {
    "VZSoldierArc_Zev01_Soldier_AI Subdual_x_x_x_x_x_01",
    "VZSoldierArc_Zev01_VZ Soldier_AI Subdual_x_x_x_x_x_08"
  }
}

function _TargetSubdued(self, uGuid)
  _TargetIntoSubdued(self)
  Pg.RemoveContextAction(uGuid)
  local uFaction = Pg.GetGuidByName(MrxUtil.GetFaction(uGuid))
  if uFaction then
    Ai.AddInfraction(Player.GetPrimaryCharacter(), uFaction, 5)
    if Player.GetSecondaryCharacter() then
      Ai.AddInfraction(Player.GetSecondaryCharacter(), uFaction, 5)
    end
  else
    Debug.Printf("No faction found???")
  end
  if Sys.IsGermanSKU() then
    Debug.Printf("CENSORED!!!!")
    _TargetExtracted(self, uGuid)
  else
    Pg.AddContextAction(uGuid, "[ContextAction.Carry]", 2)
    if self._tEvents.uStowEvent and self._tEvents.uStowEvent[uGuid] then
      return
    end
    self:_AddSupport(uGuid)
    Object.AddLabel(uGuid, "Prisoner")
    self._tEvents.uStowEvent = self._tEvents.uStowEvent or {}
    self._tEvents.uStowEvent[uGuid] = Event.Create(Event.ObjectInSeat, {
      uGuid,
      "ExtractionHelicopter",
      "a",
      "e"
    }, _TargetExtracted, {self, uGuid})
  end
  local tConfig = self:GetConfig()
  MrxUtil.ProcessCallbackTable(tConfig.tOnTargetSubdued, {uGuid})
  MrxUtil.CallWithOptionalArgs(tConfig.fOnTargetSubdued, {uGuid})
end

function _TargetExtracted(self, uGuid)
  self._bIsVerified = true
  self.nCurrHVTState = nil
  MrxTutorialManager.HideMessage()
  MrxVerifyManager.UpdateTarget(uGuid, "captured")
  if type(uGuid) == "userdata" then
    self:RemoveTarget(uGuid)
  end
  if self._tEvents.uStowEvent then
    self._tEvents.uStowEvent[uGuid] = nil
    self:_RemoveSupport(uGuid)
  end
  self:CompletePart(uGuid, false)
  Object.FadeOut(uGuid, 0.25, true)
end

function _SetHostileAttitudeChangeEvent(self, uGuid, sFaction)
  for uHeliGuid, oSupport in pairs(tActiveHelicopters) do
    if uHeliGuid and oSupport then
      MrxSupportPickup.GoHome(oSupport, uHeliGuid)
    end
  end
  tActiveHelicopters = {}
  local tConfig = self:GetConfig()
  MrxSupportData.RemoveFreebie(_GetSupportByFaction(tConfig.sFactionId))
  tHasExtractionFreebie[tConfig.sFactionId] = nil
  if self._AttitudeChangeEvent then
    Event.Delete(self._AttitudeChangeEvent)
  end
  self._AttitudeChangeEvent = MrxFactionManager.CreatePersistentAttitudeChangeEvent({
    tConfig.sFactionId,
    "Pmc"
  }, function()
    if MrxFactionManager.GetAttitudeLabel(sFaction, "Pmc") ~= "Hostile" then
      _AddSupport(self, uGuid)
      _SetNonHostileAttitudeChangeEvent(self, uGuid, sFaction)
    end
  end, {})
end

function _SetNonHostileAttitudeChangeEvent(self, uGuid, sFaction)
  local tConfig = self:GetConfig()
  MrxSupportData.AddFreebie(_GetSupportByFaction(tConfig.sFactionId))
  tHasExtractionFreebie[tConfig.sFactionId] = true
  if self._AttitudeChangeEvent then
    Event.Delete(self._AttitudeChangeEvent)
  end
  self._AttitudeChangeEvent = MrxFactionManager.CreatePersistentAttitudeChangeEvent({
    tConfig.sFactionId,
    "Pmc"
  }, function()
    if MrxFactionManager.GetAttitudeLabel(sFaction, "Pmc") == "Hostile" then
      self:_RemoveSupport(uGuid)
      _SetHostileAttitudeChangeEvent(self, uGuid, sFaction)
    end
  end, {})
end

function _AddSupport(self, uGuid)
  self._nSupportCount = self._nSupportCount + 1
  if tHVTDistanceEvents[uGuid] then
    Event.Delete(tHVTDistanceEvents[uGuid])
    tHVTDistanceEvents[uGuid] = nil
  end
  if self._nSupportCount == 1 then
    local tConfig = self:GetConfig()
    MrxSupportData.AddFreebie(_GetSupportByFaction(tConfig.sFactionId))
    tHasExtractionFreebie[tConfig.sFactionId] = true
    self:_CreateDistanceEvent(uGuid)
  end
end

function _RemoveSupport(self, uGuid)
  self._nSupportCount = self._nSupportCount - 1
  self._nSupportCount = Math.max(self._nSupportCount, 0)
  if tHVTDistanceEvents[uGuid] then
    Event.Delete(tHVTDistanceEvents[uGuid])
    tHVTDistanceEvents[uGuid] = nil
  end
  if self._nSupportCount == 0 then
    if self._AttitudeChangeEvent then
    end
    for uHeliGuid, oSupport in pairs(tActiveHelicopters) do
      if uHeliGuid and oSupport then
        MrxSupportPickup.GoHome(oSupport, uHeliGuid)
      end
    end
    tActiveHelicopters = {}
    local tConfig = self:GetConfig()
    MrxSupportData.RemoveFreebie(_GetSupportByFaction(tConfig.sFactionId))
    tHasExtractionFreebie[tConfig.sFactionId] = nil
  end
end

function _GetSupportByFaction(sFaction)
  local tSupport = {
    All = "Extraction_AL",
    Chi = "Extraction_CH",
    Gur = "Extraction_GR",
    Oil = "Extraction_OC",
    Pmc = "Extraction_PMC",
    Pir = "Extraction_PR"
  }
  return tSupport[sFaction] or tSupport.Pmc
end

function SendPlayerJoinEvents()
  Debug.Printf("SendPlayerJoinEvents: Begin")
  if not Net.IsServer() then
    return
  end
  for factionKey, hasExtractionSupport in pairs(tHasExtractionFreebie) do
    Debug.Printf("SendPlayerJoinEvents: Checking %s for extraction support", factionKey)
    if hasExtractionSupport then
      Debug.Printf("SendPlayerJoinEvents: Add freebie for %s", factionKey)
      MrxSupportData.AddFreebie(_GetSupportByFaction(factionKey), nil, Player.GetSecondaryPlayer())
    end
  end
  Debug.Printf("SendPlayerJoinEvents: End")
end

function _GetShortDescription()
  return "[Generic.ObjectiveVerify]"
end

function GetInlineIcon(self)
  local tConfig = self:GetConfig()
  if tConfig.bOptional then
    return "[objverify2]"
  else
    return "[objverify]"
  end
end

function _GetJust2DCheckNeeded()
  return true
end

function _GetTargetRadarIcon()
  return "objective_verify"
end

function _GetTargetPdaIcon(bOptional)
  if bOptional then
    return "icon_verify_2_mc"
  else
    return "icon_verify_1_mc"
  end
end

function _GetTargetGameSpaceIcon()
  return "HUD_objective_verify"
end

function _IsValidTarget(uGuid)
  local anyPlayer = Player.GetAnyCharacter()
  local allPlayers = Player.GetAllCharacters()
  if anyPlayer == uGuid then
    return true
  end
  if allPlayers == uGuid then
    return true
  end
  return Object.IsAlive(uGuid)
end

function _CreateDistanceEvent(self, uGuid)
  tHVTDistanceEvents[uGuid] = Event.Create(Event.ObjectProximity, {
    Player.GetAllCharacters(),
    uGuid,
    ">",
    150,
    false,
    true
  }, _OnHVTOutOfRange, {self, uGuid})
end

function _OnHVTOutOfRange(self, uGuid)
  Debug.Printf("You got too far from the HVT")
  self:_RemoveSupport(uGuid)
  tHVTDistanceEvents[uGuid] = Event.Create(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uGuid,
    "<",
    140,
    false,
    true
  }, _AddSupport, {self, uGuid})
end
