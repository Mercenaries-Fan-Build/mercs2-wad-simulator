inherit("MrxTaskObjective")
import("MrxFollow")
import("MrxUtil")
import("MrxTutorialManager")

function Activated(self)
  MrxTaskObjective.Activated(self)
  local tConfig = self:GetConfig()
  if type(tConfig.vDestLoc) == "string" then
    tConfig.vDestLoc = Pg.GetGuidByName(tConfig.vDestLoc)
  end
  if type(tConfig.vDestRegion) == "string" then
    tConfig.vDestRegion = Pg.GetGuidByName(tConfig.vDestRegion)
  end
  tConfig.fDist = MrxUtil.SetDefault(tConfig.fDist, 5)
  tConfig.bStop = MrxUtil.SetDefault(tConfig.bStop, true)
  tConfig.bDetach = MrxUtil.SetDefault(tConfig.bDetach, tConfig.bStop)
  tConfig.bXZOnly = MrxUtil.SetDefault(tConfig.bXZOnly, false)
  tConfig.bUseDestRing = MrxUtil.SetDefault(tConfig.bUseDestRing, tConfig.bStop and tConfig.vDestLoc ~= nil)
  tConfig.bHumansFollow = MrxUtil.SetDefault(tConfig.bHumansFollow, true)
  tConfig.bDisplayHelpText = MrxUtil.SetDefault(tConfig.bDisplayHelpText, true)
  local uGuid = ObjectFilter.GetCoopPlayerGuid(self._uTgtObjFilter)
  local tTargets = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  uGuid = uGuid or tTargets[1]
  if uGuid == Player.GetAnyCharacter() then
    self._tEvents.uProxEvent = self:_PlayerDeliveryCreate(uGuid)
    self._tEvents.uDeathEvent = Event.CreatePersistent(Event.ObjectDeath, {
      Player.GetAnyCharacter()
    }, function()
      local bHeroesAlive = false
      local tPlayers = Player.GetAllPlayers()
      for _, uPlayer in ipairs(tPlayers) do
        local uCharacter = Player.GetCharacter(uPlayer)
        if uCharacter and Object.IsAlive(uCharacter) then
          bHeroesAlive = true
        end
      end
      if not bHeroesAlive then
        _OnStatusChange(self, "destroyed")
      end
    end)
  elseif uGuid == Player.GetAllCharacters() then
    self._tEvents.uProxEvent = self:_PlayerDeliveryCreate(uGuid)
    self._tEvents.uDeathEvent = Event.CreatePersistent(Event.ObjectDeath, {
      Player.GetAnyCharacter()
    }, _OnStatusChange, {self, "destroyed"})
  elseif table.getn(tTargets) == 0 then
    self._tEvents.uProxEvent = self:LabelFilterDeliveryCreate(tConfig)
  else
    self._iNumAttached = 0
    for i, uGuid in pairs(tTargets) do
      if Object.HasLabel(uGuid, "human") then
        if not Object.IsPlayerControlled(uGuid) then
          self:_HumanDeliveryCreate(uGuid)
        end
      elseif Object.HasLabel(uGuid, "vehicle") then
        self:_VehicleDeliveryCreate(uGuid)
      else
        self:_ObjectDeliveryCreate(uGuid)
      end
    end
    self._tEvents.uDeathEvent = Event.CreatePersistent(Event.ObjectDeath, {
      self._uTgtObjFilter
    }, _OnStatusChange, {self, "destroyed"})
  end
end

function Cleanup(self)
  if self.uRing then
    Marker.Remove(self.uRing)
    if Net.IsServer() then
      Net.SendEvent_RemoveMarkerObjective(self.uRing)
    end
    self.uRing = nil
  end
  for i, tTargetData in pairs(self._tTargets) do
    self:_CleanupTargetEvents(tTargetData)
  end
  MrxTaskObjective.Cleanup(self)
end

function _CleanupTargetEvents(self, tTargetData)
  if tTargetData == nil then
    return
  end
  if tTargetData.oFollower then
    tTargetData.oFollower:Activate(false)
    tTargetData.oFollower = nil
  end
  if type(tTargetData.tEvents) == "table" then
    for j, e in pairs(tTargetData.tEvents) do
      Event.Delete(e)
    end
  end
  tTargetData.tEvents = nil
end

function _DeliveryCheck(self, uGuid)
  local tConfig = self:GetConfig()
  if tConfig.fEvaluateTarget and not tConfig.fEvaluateTarget(uGuid) then
    return false
  end
  if tConfig.bStop and tConfig.bDetach then
    local bDelivered
    if Object.HasLabel(uGuid, "human") then
      bDelivered = self:_HumanDeliveryCheck(uGuid, tConfig.bStop)
    elseif Object.HasLabel(uGuid, "vehicle") then
      bDelivered = self:_VehicleDeliveryCheck(uGuid, tConfig.bStop)
    else
      bDelivered = self:_ObjectDeliveryCheck(uGuid, tConfig.bStop)
    end
    if bDelivered then
      self:_TargetDelivered(uGuid)
      if self:GetConfig().bDisplayHelpText then
        MrxTutorialManager.HideMessage()
        Net.SendCustomEvent("MrxTaskObjectiveDeliver", NETEVENT_CLEARTUTORIAL, {})
      end
    end
    return bDelivered
  else
    self:_TargetDelivered(uGuid)
    return true
  end
end

function _TargetDelivered(self, uGuid)
  self:_CleanupTargetEvents(self._tTargets[uGuid])
  self:RemoveTarget(uGuid)
  self:CompletePart(uGuid)
end

function _OnAttachment(self, sAttachMode, iGuid, bAttached)
  Debug.Printf("OnAttachment change: ", iGuid, " ", sAttachMode, " status: ", bAttached)
  local tConfig = self:GetConfig()
  if bAttached then
    self._iNumAttached = self._iNumAttached + 1
  else
    self._iNumAttached = self._iNumAttached - 1
  end
  if self._iNumAttached == 1 then
    self:EnableDestinationBlip(true)
  elseif self._iNumAttached == 0 then
    self:EnableDestinationBlip(false)
  end
  self:_SetTargetStatus(iGuid, not bAttached)
  if tConfig.fAttachCallback then
    if type(tConfig.tAttachCallbackData) == "table" then
      tConfig.fAttachCallback(unpack(tConfig.tAttachCallbackData), iGuid, sAttachMode, bAttached)
    else
      tConfig.fAttachCallback(iGuid, sAttachMode, bAttached)
    end
  end
end

function _OnStatusChange(self, sStatusType, iGuid)
  Debug.Printf("_OnStatusChange: ", iGuid, " ", sStatusType)
  local tConfig = self:GetConfig()
  if tConfig.fStatusChangeCallback then
    if type(tConfig.tStatusChangeCallbackData) == "table" then
      tConfig.fStatusChangeCallback(unpack(tConfig.tStatusChangeCallbackData), iGuid, sStatusType)
    else
      tConfig.fStatusChangeCallback(iGuid, sStatusType)
    end
  end
  if iGuid then
    local tGuids = self._tTargets
    if tGuids[iGuid] and sStatusType == "destroyed" then
      if tGuids[iGuid].bStatus == false then
        self._iNumAttached = self._iNumAttached - 1
        if self._iNumAttached == 0 then
          self:EnableDestinationBlip(false)
        end
      end
      self:_CleanupTargetEvents(tGuids[iGuid])
      self:RemoveTarget(iGuid)
    end
    self:_SetTargetStatus(iGuid, false)
  end
  self:CancelPart()
end

function _TargetLeftDestination(self, uGuid)
  local tConfig = self:GetConfig()
  local tTargetData = self._tTargets[uGuid]
  tTargetData.bAtDestination = false
  local nEventType, tEventParams
  if tConfig.vDestRegion then
    nEventType = Event.Boundary
    tEventParams = {
      uGuid,
      tConfig.vDestRegion,
      "enter",
      tConfig.bStop
    }
  elseif tConfig.vDestLoc then
    nEventType = Event.ObjectProximity
    tEventParams = {
      uGuid,
      tConfig.vDestLoc,
      "<",
      tConfig.fDist,
      tConfig.bStop,
      tConfig.bXZOnly
    }
  end
  tTargetData.tEvents.eDeliver = Event.Create(nEventType, tEventParams, _TargetAtDestination, {self, uGuid})
  if tConfig.bDisplayHelpText then
    MrxTutorialManager.HideMessage()
    Net.SendCustomEvent("MrxTaskObjectiveDeliver", NETEVENT_CLEARTUTORIAL, {})
  end
end

function _TargetAtDestination(self, uGuid)
  local tConfig = self:GetConfig()
  local tTargetData = self._tTargets[uGuid]
  tTargetData.bAtDestination = true
  local nEventType, tEventParams
  if tConfig.vDestRegion then
    nEventType = Event.Boundary
    tEventParams = {
      uGuid,
      tConfig.vDestRegion,
      "exit",
      false
    }
  elseif tConfig.vDestLoc then
    nEventType = Event.ObjectProximity
    tEventParams = {
      uGuid,
      tConfig.vDestLoc,
      ">",
      tConfig.fDist,
      false,
      tConfig.bXZOnly
    }
  end
  tTargetData.tEvents.eDeliver = Event.Create(nEventType, tEventParams, _TargetLeftDestination, {self, uGuid})
  self:_DeliveryCheck(uGuid)
end

function LabelFilterDeliveryCreate(self, tConfig)
  self:EnableDestinationBlip(true)
  local nEventType, tEventParams
  if tConfig.vDestRegion and not tConfig.vDestLoc then
    Debug.Printf("WARNING: Cannot deliver label filter to region")
    return
  end
  return Event.CreatePersistent(Event.ObjectProximity, {
    self._uTgtObjFilter,
    tConfig.vDestLoc,
    "<",
    tConfig.fDist,
    tConfig.bStop,
    tConfig.bXZOnly
  }, function(self, tObj)
    if type(tObj) == "table" then
      for i, uGuid in pairs(tObj) do
        self:_FilterTargetAtDestination(uGuid)
        if self._nCompleted == self._nQuota then
          break
        end
      end
    else
      self:_FilterTargetAtDestination(tObj)
    end
  end, {self})
end

function _FilterTargetAtDestination(self, uGuid)
  ObjectFilter.AddObject(self._uTgtObjFilter, uGuid, true)
  local tTargetData = {}
  self._tTargets[uGuid] = tTargetData
  tTargetData.tEvents = {}
  tTargetData.bIsLabelFilter = true
  if not Object.HasLabel(uGuid, "human") then
    tTargetData.bWinched = Object.IsWinched(uGuid) ~= nil
    tTargetData.tEvents.eWinch = Event.CreatePersistent(Event.ObjectWinched, {
      uGuid,
      0,
      "any"
    }, self._OnObjectWinched, {self})
  end
  if Object.HasLabel(uGuid, "vehicle") then
    self:_OnPlayerInVehicle(nil, uGuid, nil, nil, "exit")
  end
  tTargetData.bAtDestination = true
  if not self:_DeliveryCheck(uGuid) then
    local tConfig = self:GetConfig()
    tTargetData.tEvents.eDeliver = Event.Create(Event.ObjectProximity, {
      uGuid,
      tConfig.vDestLoc,
      ">",
      tConfig.fDist,
      false,
      tConfig.bXZOnly
    }, _FilterTargetLeftDestination, {self, uGuid})
  end
end

function _FilterTargetLeftDestination(self, uGuid)
  self:_CleanupTargetEvents(self._tTargets[uGuid])
  self._tTargets[uGuid] = nil
  ObjectFilter.RemoveObject(self._uTgtObjFilter, uGuid)
  if self:GetConfig().bDisplayHelpText then
    MrxTutorialManager.HideMessage()
    Net.SendCustomEvent("MrxTaskObjectiveDeliver", NETEVENT_CLEARTUTORIAL, {})
  end
end

sGlobalDiscCount = 1

function EnableDestinationBlip(self, bOn)
  local tConfig = self:GetConfig()
  self:_SetTargetStatus(tConfig.vDestLoc or tConfig.vDestRegion, bOn, "destination")
  if tConfig.bUseDestRing then
    if bOn then
      if not self.uRing then
        local r, g, b
        if tConfig.bOptional then
          r, g, b = MrxUtil.GetSecondaryObjectiveRgb()
        else
          r, g, b = MrxUtil.GetPrimaryObjectiveRgb()
        end
        self.uRing = Marker.AddDisc(tConfig.vDestLoc, tConfig.fDist, r, g, b, 0.02)
        if Net.IsServer() then
          Net.SendEvent_AddMarkerObjective(tConfig.vDestLoc, self.uRing, r, g, b, 0.02, 0, tConfig.fDist, 0, true)
        end
        if sGlobalDiscCount >= 8192 then
          sGlobalDiscCount = 0
        end
        self.discCount = sGlobalDiscCount
        sGlobalDiscCount = sGlobalDiscCount + 1
      end
    elseif self.uRing then
      Marker.Remove(self.uRing)
      if Net.IsServer() then
        Net.SendEvent_RemoveMarkerObjective(self.uRing)
      end
      self.uRing = nil
    end
  end
end

function EnableTargetBlips(self, bOn)
  local tTgtInclude = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  for _, iTarget in pairs(tTgtInclude) do
    self:_SetTargetStatus(iTarget, bOn)
  end
end

function _PlayerDeliveryCreate(self, uGuid)
  local tConfig = self:GetConfig()
  local nEventType, tEventParams
  if tConfig.vDestRegion then
    nEventType = Event.Boundary
    tEventParams = {
      uGuid,
      tConfig.vDestRegion,
      "enter",
      tConfig.bStop
    }
  elseif tConfig.vDestLoc then
    nEventType = Event.ObjectProximity
    tEventParams = {
      uGuid,
      tConfig.vDestLoc,
      "<",
      tConfig.fDist,
      tConfig.bStop,
      tConfig.bXZOnly
    }
  end
  self:EnableDestinationBlip(true)
  return Event.Create(nEventType, tEventParams, _TargetDelivered, {self, uGuid})
end

function _HumanDeliveryCreate(self, uGuid)
  local tConfig = self:GetConfig()
  local tTargetData = self._tTargets[uGuid]
  local tEvents = {}
  if tConfig.bHumansFollow then
    if not tConfig.uStartAttachedToPlayer then
      self._iNumAttached = self._iNumAttached + 1
    end
    local tFollowerConfig = {
      _vActor = uGuid,
      _vObjectToFollow = tConfig.uStartAttachedToPlayer,
      _fCallback = _OnAttachment,
      _tCallbackData = {self, "follow"},
      tStartFollowVO = tConfig.tStartFollowVO,
      tStopFollowVO = tConfig.tStopFollowVO,
      tLostVO = tConfig.tLostVO,
      tFoundVO = tConfig.tFoundVO,
      tHostileVO = tConfig.tHostileVO,
      tHostileRecoveredVO = tConfig.tHostileRecoveredVO
    }
    local oFollower = MrxFollow:Create(tFollowerConfig)
    oFollower:Activate(true, tConfig.uStartAttachedToPlayer ~= nil)
    tTargetData.oFollower = oFollower
  end
  tEvents.eSubdue = Event.Create(Event.HumanStateTransition, {
    uGuid,
    "*",
    "subdued.idle"
  }, _OnStatusChange, {self, "subdued"})
  tTargetData.tEvents = tEvents
  self:_TargetLeftDestination(uGuid)
end

function _HumanDeliveryCheck(self, uGuid)
  if Object.InSeat(uGuid) and not Object.IsPlayerControlled(uGuid) then
    local uVeh = Vehicle.GetFromRider(uGuid)
    Vehicle.Exit(uVeh, uGuid)
  end
  local tTargetData = self._tTargets[uGuid]
  if tTargetData and not Object.IsPlayerControlled(uGuid) then
    return tTargetData.bAtDestination
  else
    return true
  end
end

function _HumanOnAttachment(self, sAttachMode, iGuid, bAttached)
  self:_OnAttachment(sAttachMode, iGuid, bAttached)
  self:_MarkAttachedHuman(iGuid, bAttached)
end

function _MarkAttachedHuman(self, uGuid, bEnable)
  local tTargetData = self._tTargets[uGuid]
  if not tTargetData then
    return
  end
  if bEnable then
    local r = 255
    local g = 255
    local b = 0
    tTargetData.uMarker = Marker.Add(0, 2, 0, uGuid, r, g, b, 0.05)
    if Net.IsServer() then
      Net.SendEvent_AddMarkerObjective(uGuid, tTargetData.uMarker, r, g, b, 2, "", 0.05, 0)
    end
    Hud.Radar:AddObjective({
      sName = tostring(uGuid),
      nR = r,
      nG = g,
      nB = b,
      nWidth = 2,
      nHeight = 2,
      uGuid = uGuid,
      bSticky = true
    })
    Net.SendEvent_AddRadarObjective(tostring(uGuid), 0, 2, 0, r, g, b, 2, 2, "", uGuid, true, false, 0)
  elseif tTargetData.uMarker ~= nil then
    Marker.Remove(tTargetData.uMarker)
    Hud.Radar:RemoveObjective({
      sName = tostring(uGuid)
    })
    if Net.IsServer() then
      Net.SendEvent_RemoveMarkerObjective(tTargetData.uMarker)
      Net.SendEvent_RemoveRadarObjective(tostring(uGuid))
    end
  end
end

function _ObjectDeliveryCreate(self, uGuid)
  local tEvents = {}
  tEvents.eWinch = Event.CreatePersistent(Event.ObjectWinched, {
    uGuid,
    0,
    "any"
  }, self._OnObjectWinched, {self})
  local tTargetData = self._tTargets[uGuid]
  tTargetData.tEvents = tEvents
  tTargetData.bWinched = false
  local uWincher = Object.IsWinched(uGuid)
  if uWincher then
    self:_OnObjectWinched(uGuid, uWincher, "attach")
  end
  self:_TargetLeftDestination(uGuid)
end

function _ObjectDeliveryCheck(self, uGuid)
  local bWinched
  local tTargetData = self._tTargets[uGuid]
  if tTargetData.bWinched then
    if self:GetConfig().bDisplayHelpText then
      MrxTutorialManager.ShowMessage("[objective.Deliver.winch]")
      Net.SendCustomEvent("MrxTaskObjectiveDeliver", NETEVENT_UNWINCH, {})
    end
    return false
  else
    return true
  end
end

function _VehicleDeliveryCreate(self, uGuid)
  self:_ObjectDeliveryCreate(uGuid)
  local tTargetData = self._tTargets[uGuid]
  tTargetData.bPlayerInVehicle = false
  self:_OnPlayerInVehicle(nil, uGuid, nil, nil, "exit")
end

function _VehicleDeliveryCheck(self, uGuid)
  local bPlayerInVehicle
  local tTargetData = self._tTargets[uGuid]
  if tTargetData.bPlayerInVehicle then
    if self:GetConfig().bDisplayHelpText and Net.IsServer() then
      MrxTutorialManager.ShowMessage("[objective.Deliver.vehicleExit]")
      Net.SendCustomEvent("MrxTaskObjectiveDeliver", NETEVENT_EXITVEHICLE, {})
    end
    return false
  else
    return self:_ObjectDeliveryCheck(uGuid)
  end
end

function _OnObjectWinched(self, uObject, uWincher, sState)
  Debug.Printf("OBJECT WINCHED state ", uObject, " ", uWincher, " / ", sState)
  local tTargetData = self._tTargets[uObject]
  local bWinched = sState == "attach"
  if tTargetData.bWinched ~= bWinched then
    tTargetData.bWinched = bWinched
    if not tTargetData.bPlayerInVehicle and not tTargetData.bIsLabelFilter then
      self:_OnAttachment("winched", uObject, tTargetData.bWinched)
    end
  end
  if tTargetData.bAtDestination then
    self:_DeliveryCheck(uObject)
  end
end

function _OnPlayerInVehicle(self, uPlayerChar, uVehicle, sSeatType, uSeat, sAction)
  local bEnter = sAction == "enter"
  if not bEnter then
    local tRiders = Vehicle.GetRiders(uVehicle)
    for _, uRider in pairs(tRiders) do
      if Object.IsPlayerControlled(uRider) then
        Debug.Printf("!!!!!!!! a player found in vehicle still")
        bEnter = true
        uPlayerChar = uRider
      end
    end
  end
  local tTargetData = self._tTargets[uVehicle]
  if tTargetData.bPlayerInVehicle ~= bEnter then
    tTargetData.bPlayerInVehicle = bEnter
    if not tTargetData.bWinched and not tTargetData.bIsLabelFilter then
      self:_OnAttachment("in_target_veh", uVehicle, bEnter)
    end
  end
  Event.Delete(tTargetData.tEvents.eVehicleSeat)
  Event.Delete(tTargetData.tEvents.eMPDrop)
  if bEnter then
    tTargetData.tEvents.eVehicleSeat = Event.Create(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      uVehicle,
      "a",
      "x"
    }, _OnPlayerInVehicle, {self})
    local uPlayer = Object.IsPlayerControlled(uPlayerChar)
    if not Player.IsLocal(uPlayer) then
      tTargetData.tEvents.eMPDrop = Event.Create(Event.ScriptEvent, {
        "mpPlayerLeft",
        function(tData)
          return uPlayerChar == tData[2]
        end
      }, _OnPlayerInVehicle, {
        self,
        uPlayerChar,
        uVehicle,
        "any",
        uVehicle,
        "exit"
      })
    end
  else
    tTargetData.tEvents.eVehicleSeat = Event.Create(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      uVehicle,
      "a",
      "e"
    }, _OnPlayerInVehicle, {self})
  end
  if tTargetData.bAtDestination then
    self:_DeliveryCheck(uVehicle)
  end
end

function _GetShortDescription()
  return "[Generic.ObjectiveDeliver]"
end

function GetInlineIcon(self)
  local tConfig = self:GetConfig()
  if tConfig.bOptional then
    return "[objdeliver2]"
  else
    return "[objdeliver]"
  end
end

function _GetTargetRadarIcon()
  return "objective_action"
end

function _GetTargetPdaIcon(bOptional)
  if bOptional then
    return "icon_action_2_mc"
  else
    return "icon_action_1_mc"
  end
end

function _GetTargetGameSpaceIcon()
  return "HUD_objective_action"
end

function _GetDestinationRadarIcon()
  return "objective_deliverable"
end

function _GetDestinationPdaIcon(bOptional)
  if bOptional then
    return "icon_deliverable_2_mc"
  else
    return "icon_deliverable_1_mc"
  end
end

function _GetDestinationGameSpaceIcon()
  return "HUD_objective_deliverable"
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

NETEVENT_EXITVEHICLE = 0
NETEVENT_UNWINCH = 1
NETEVENT_CLEARTUTORIAL = 5

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_EXITVEHICLE then
    MrxTutorialManager.ShowMessage("[objective.Deliver.vehicleExit]")
  elseif nEventType == NETEVENT_UNWINCH then
    MrxTutorialManager.ShowMessage("[objective.Deliver.winch]")
  elseif nEventType == NETEVENT_CLEARTUTORIAL then
    MrxTutorialManager.HideMessage()
  end
end
