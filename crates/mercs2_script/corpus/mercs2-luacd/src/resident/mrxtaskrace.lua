inherit("MrxTask")
import("MrxGuiHudMessage")
import("MrxStatsManager")
NETEVENT_MARKLOC = 0
NETEVENT_UNMARKLOC = 1
NETEVENT_MARKFINISH = 2
kTYPE_GATE = 1
kTYPE_RING = 2
_knWldBlpNearDist = 200
_knWldBlpFarDist = 300
tNextLocVals = {}
tCurLocVals = {}

function NetEventCallback(eventId, tArgs)
  local nLoc
  if eventId == NETEVENT_MARKLOC then
    nLoc = tArgs[5]
    tCurLocVals[nLoc] = MarkCurCourseLoc(tArgs[1], tArgs[3], tArgs[4], false)
    tNextLocVals[nLoc] = MarkNextCourseLoc(tArgs[2], tArgs[3], tArgs[4])
  elseif eventId == NETEVENT_UNMARKLOC then
    nLoc = tArgs[1]
    if tCurLocVals[nLoc] then
      UnmarkCourseLoc(tCurLocVals[nLoc])
      tCurLocVals[nLoc] = nil
    end
    if tNextLocVals[nLoc] then
      UnmarkCourseLoc(tNextLocVals[nLoc])
      tNextLocVals[nLoc] = nil
    end
  elseif eventId == NETEVENT_MARKFINISH then
    nLoc = tArgs[4]
    tCurLocVals[nLoc] = MarkCurCourseLoc(tArgs[1], tArgs[2], tArgs[3], true)
  end
end

function Activated(self)
  local tConfig = self:GetConfig()
  self.fWidth = MrxUtil.SetDefault(tConfig.fWidth, 10)
  self.bUseTripWires = MrxUtil.SetDefault(tConfig.bUseTripWires, true)
  if self.bUseTripWires then
    if tConfig.sGateType == "ring" then
      self.iGateType = kTYPE_RING
    else
      self.iGateType = kTYPE_GATE
    end
  end
  self.vTgtInclude = MrxUtil.SetDefault(tConfig.vTgtInclude, Player.GetAnyCharacter())
  self.nAddTime = tConfig.nAddTime
  self._tCourseLocs = tConfig.tCourseLocs
  local tTimerParams = tConfig.tTimerParams
  if tTimerParams then
    tTimerParams.bTaskManualStart = true
  end
  MrxTask.Activated(self)
  self.iCurLoc = 0
  if self.vTgtInclude == Player.GetAnyCharacter() then
    self:_StartRace()
  else
    if type(self.vTgtInclude) == "table" then
      for i, veh in pairs(self.vTgtInclude) do
        if Object.IsPlayerControlled(veh) then
          self:_StartRace()
          return
        end
      end
    elseif Object.IsPlayerControlled(self.vTgtInclude) then
      self:_StartRace()
      return
    end
    self:CreateChild({
      sName = "Enter car",
      sModuleName = "MrxTaskObjectiveEnterVehicle",
      vTgtInclude = self.vTgtInclude,
      nQuota = 1,
      fOnComplete = function()
        self:_StartRace()
      end,
      fOnCancel = function()
        self:Cancel()
      end,
      fStatusChangeCallback = _OnStatusChange,
      tStatusChangeCallbackData = {self},
      vVoSeqOnAdd = tConfig.vVoSeqOnAdd
    })
    tConfig.vVoSeqOnAdd = nil
  end
end

function Cleanup(self)
  self:UnmarkLocation()
  self._uTgtObjFilter = nil
  MrxTask.Cleanup(self)
end

function GetWinner(self)
  return self._uWinner
end

function _SetupDestination(self)
  self:UnmarkLocation()
  self.iCurLoc = self.iCurLoc + 1
  local uCurPoint = Pg.GetGuidByName(self._tCourseLocs[self.iCurLoc])
  local uNextPoint
  if self._tCourseLocs[self.iCurLoc + 1] then
    uNextPoint = Pg.GetGuidByName(self._tCourseLocs[self.iCurLoc + 1])
  end
  local tConfig = self:GetConfig()
  self:CreateChild({
    sName = "Loc" .. self.iCurLoc,
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = self.vTgtInclude,
    nQuota = 1,
    vDestLoc = uCurPoint,
    sDspShortDesc = _GetDspShortDesc(self, self.iCurLoc),
    fDist = self.fWidth,
    bDspMsg = false,
    bDspBlpWld = not self.bUseTripWires or uNextPoint,
    nDspBlpWldNearDist = _knWldBlpNearDist,
    nDspBlpWldFarDist = _knWldBlpFarDist,
    bStop = false,
    bXZOnly = false,
    fOnPartComplete = function(uGuid)
      if not uNextPoint and Object.IsPlayerControlled(uGuid) then
        _uDriver = Vehicle.GetDriver(uGuid)
        if _uDriver == Player.GetPrimaryCharacter() then
          self._uWinner = Player.GetPrimaryPlayer()
        elseif _uDriver == Player.GetSecondaryCharacter() then
          self._uWinner = Player.GetSecondaryPlayer()
        end
      end
    end,
    fOnComplete = function()
      if uNextPoint then
        self:_SetupDestination()
        if self._oTimer and self.nAddTime then
          self._oTimer:AddTime(self.nAddTime)
        end
      else
        self:_FinishRace()
      end
      Sound.CueSound(0, "ui_HUD_Objective_Complete")
    end,
    fOnCancel = function()
      self:Cancel()
    end,
    vVoSeqOnAdd = tConfig.vVoSeqOnAdd,
    fStatusChangeCallback = _OnStatusChange,
    tStatusChangeCallbackData = {self}
  })
  tConfig.vVoSeqOnAdd = nil
  if uNextPoint then
    self.tCurMarker = MarkCurCourseLoc(uCurPoint, self.iGateType, self.fWidth, false)
    self.tNextMarker = MarkNextCourseLoc(uNextPoint, self.iGateType, self.fWidth)
    if Net.IsServer() then
      Net.SendCustomEvent("MrxTaskRace", NETEVENT_MARKLOC, {
        uCurPoint,
        uNextPoint,
        self.iGateType,
        self.fWidth,
        self.iCurLoc
      })
    end
  else
    self.tCurMarker = MarkCurCourseLoc(uCurPoint, self.iGateType, self.fWidth, true)
    if Net.IsServer() then
      Net.SendCustomEvent("MrxTaskRace", NETEVENT_MARKFINISH, {
        uCurPoint,
        self.iGateType,
        self.fWidth,
        self.iCurLoc
      })
    end
  end
end

function _GetDspShortDesc(self, nLoc)
  if type(nLoc) ~= "number" then
    Debug.Printf("ERROR: Bad location # received")
  end
  local nNextLoc = nLoc + 1
  if self._tCourseLocs[nNextLoc] then
    return "[Objective.Race.Checkpoint]" .. " (" .. nLoc - 1 .. "/" .. tostring(table.getn(self._tCourseLocs)) .. ")"
  else
    return "[Objective.Race.Finish]"
  end
end

function UnmarkLocation(self)
  if self.tCurMarker then
    UnmarkCourseLoc(self.tCurMarker)
    self.tCurMarker = nil
  end
  if self.tNextMarker then
    UnmarkCourseLoc(self.tNextMarker)
    self.tNextMarker = nil
  end
  if Net.IsServer() then
    Net.SendCustomEvent("MrxTaskRace", NETEVENT_UNMARKLOC, {
      self.iCurLoc
    })
  end
end

function MarkCurCourseLoc(uGuid, iGateType, fWidth, bFinish)
  if not iGateType then
    return
  end
  local r, g, b = MrxUtil.GetPrimaryObjectiveRgb()
  if iGateType == kTYPE_GATE then
    retGate, retFinish = _DrawTripWire(uGuid, fWidth, r, g, b, bFinish)
  elseif iGateType == kTYPE_RING then
    retGate, retFinish = _DrawRing(uGuid, fWidth, r, g, b, bFinish)
  end
  return {uGate = retGate, uFinish = retFinish}
end

function MarkNextCourseLoc(uGuid, iGateType, fWidth)
  local retGate, retWldBlip, retMarkerString
  local r, g, b = MrxUtil.GetSecondaryObjectiveRgb()
  if iGateType == kTYPE_GATE then
    retGate = _DrawTripWire(uGuid, fWidth, r, g, b, false)
  elseif iGateType == kTYPE_RING then
    retGate = _DrawRing(uGuid, fWidth, r, g, b, false)
  end
  retWldBlip = Marker.AddBlip(uGuid, "HUD_objective_deliverable", 32, r, g, b, 255, 1, _knWldBlpNearDist, _knWldBlpFarDist)
  retMarkerString = "NextMarker"
  Hud.Radar:AddObjective({
    sName = retMarkerString,
    nR = r,
    nG = g,
    nB = b,
    nWidth = 8,
    nHeight = 8,
    sTexture = "objective_deliverable",
    uGuid = uGuid,
    bSticky = true,
    nSortOrder = 5
  })
  return {
    uGate = retGate,
    uWldBlip = retWldBlip,
    sMarkerName = retMarkerString
  }
end

function UnmarkCourseLoc(tMarkerData)
  if tMarkerData.uGate then
    Marker.Remove(tMarkerData.uGate)
    if tMarkerData.uFinish then
      Marker.Remove(tMarkerData.uFinish)
    end
  end
  if tMarkerData.uWldBlip then
    Marker.Remove(tMarkerData.uWldBlip)
  end
  if tMarkerData.sMarkerName then
    Hud.Radar:RemoveObjective({
      sName = tMarkerData.sMarkerName
    })
  end
end

function _StartRace(self)
  local tConfig = self:GetConfig()
  if type(tConfig.tTimerParams) == "table" then
    self._oTimer = MrxTimer:Create(tConfig.tTimerParams)
    self._oTimer:Start()
    self._uTimeStamp = Sys.MainTimeStamp()
    Sys.TimeStampMark(self._uTimeStamp)
  end
  self:_SetupDestination()
end

function _FinishRace(self)
  local tConfig = self:GetConfig()
  if tConfig.sRaceMission then
    local nTime = Sys.TimeStampGetElapsed(self._uTimeStamp)
    MrxStatsManager.RecordBestTime(tConfig.sRaceMission, nTime)
  end
  self._uTimeStamp = nil
  self:Complete()
end

function _OnStatusChange(self, uGuid, sReason)
  if type(self.vTgtInclude) == "table" and #self.vTgtInclude > 1 then
    for i, guid in pairs(self.vTgtInclude) do
      if guid == uGuid then
        table.remove(self.vTgtInclude, i)
        break
      end
    end
  else
    local tConfig = self:GetConfig()
    if tConfig.fVehiclesDestroyedCallback then
      if type(tConfig.tVehiclesDestroyedCallbackData) == "table" then
        tConfig.fVehiclesDestroyedCallback(unpack(tConfig.tVehiclesDestroyedCallbackData), iGuid, sStatusType)
      else
        tConfig.fVehiclesDestroyedCallback(iGuid, sStatusType)
      end
    end
  end
end

function _DrawTripWire(uGuid, fWidth, r, g, b, bFinish)
  local x0, y0, z0 = Object.GetPosition(uGuid)
  local yaw0 = Object.GetYaw(uGuid)
  local uMarker, uFinish
  if x0 then
    uMarker = Marker.AddTripwire(x0, y0, z0, fWidth, yaw0, r, g, b)
    if bFinish then
      uFinish = Marker.Add3D(uGuid, "global_tripwirefinish", r, g, b)
    end
  end
  return uMarker, uFinish
end

function _DrawRing(uGuid, fWidth, r, g, b, bFinish)
  local uMarker, uFinish
  uMarker = Marker.Add3D(uGuid, "global_airring", r, g, b, fWidth)
  if bFinish then
    uFinish = Marker.Add3D(uGuid, "global_tripwirefinish", r, g, b)
  end
  return uMarker, uFinish
end
