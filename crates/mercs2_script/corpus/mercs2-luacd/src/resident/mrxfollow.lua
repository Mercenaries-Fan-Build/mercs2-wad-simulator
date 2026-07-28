import("MrxVoSequence")

function Create(mModule, self)
  self = self or {}
  setmetatable(self, {__index = mModule})
  self._tEvents = {}
  self.iStartVOIdx = 1
  self.iStopVOIdx = 1
  self.iLostVOIdx = 1
  self.iFoundVOIdx = 1
  self.iHostileVOIdx = 1
  self.iHostileRecoveredVOIdx = 1
  return self
end

function SetActor(self, vActor)
  self._vActor = vActor
end

function SetObjectToFollow(self, vObjectToFollow)
  self._vObjectToFollow = vObjectToFollow
end

function SetCallback(self, fCallback, tData)
  self._fCallback = fCallback
  self._tCallbackData = tData
end

function Activate(self, bEnable, bStartInFollowState)
  if bEnable then
    if self._vActor then
      self._vObjectToFollow = self._vObjectToFollow or Player.GetLocalCharacter()
      self.bVOOverride = true
      self:_Follow(bStartInFollowState, self._vObjectToFollow)
    end
  else
    self.bVOOverride = true
    self:_ToggleFollowingBehavior(false)
    self:_RemoveContextAction()
    for i, e in pairs(self._tEvents) do
      Event.Delete(e)
      self._tEvents[i] = nil
    end
  end
end

function _Follow(self, bEnable, vObjectToFollow)
  self:_ToggleFollowingBehavior(bEnable, vObjectToFollow)
  self:_ToggleContextAction(bEnable)
end

function _ToggleFollowingBehavior(self, bEnable, vObjectToFollow)
  local kMaxFollowDistance = 30
  local vActor = self._vActor
  local uGuid = self._GetActorGuid(vActor)
  local uTarget = self._GetActorGuid(vObjectToFollow)
  if bEnable then
    Debug.Printf(tostring(Object.GetName(uGuid)) .. " following ON " .. tostring(uTarget))
    Ai.LivingWorld({
      AIGuid = uGuid,
      Attrib = "LivingWorldBehaviour",
      State = false
    })
    local aiFeeling = Ai.GetFeeling(uGuid, uTarget)
    ASSERT(aiFeeling, " !!! NO AI FEELING between ", uGuid, " / ", uTarget)
    if aiFeeling and aiFeeling < 0 then
      Ai.SetFeeling(uGuid, uTarget, 100)
    end
    local h = Ai.Role({
      AIGuid = uGuid,
      Role = "Follow",
      Target = uTarget,
      MinDistance = 2,
      MaxDistance = kMaxFollowDistance,
      MoveDistance = 4,
      Priority = "hiPri",
      HardPriority = true,
      Callback = _OnFollowerCanceled,
      CallbackData = {self}
    })
    self._vObjectToFollow = uTarget
    Event.Delete(self._tEvents.eCloseEnough)
    self._tEvents.eCloseEnough = nil
    self._tEvents.ePlayer = Event.Create(Event.ScriptEvent, {
      "mpPlayerLeft",
      function(tData)
        return self._vObjectToFollow == tData[2]
      end
    }, _Follow, {
      self,
      false,
      nil
    })
    self._tEvents.eTransit = Event.Create(Event.ScriptEvent, {
      "transitStart",
      _TransitEvalFn
    }, _OnTransitStart, {self})
    if not self.bVOOverride then
      self.iStartVOIdx = self:_PlayVO(self.tStartFollowVO, self.iStartVOIdx)
    end
  else
    Debug.Printf(tostring(Object.GetName(uGuid)) .. " following OFF")
    if self._tEvents.ePlayer then
      Event.Delete(self._tEvents.ePlayer)
      self._tEvents.ePlayer = nil
    end
    if self._tEvents.eTransit then
      Event.Delete(self._tEvents.eTransit)
      self._tEvents.eTransit = nil
    end
    self._aiRole = Ai.Role({
      AIGuid = uGuid,
      Role = "Idle",
      Priority = "hiPri",
      Callback = nil,
      CallbackData = nil
    })
    if not self.bVOOverride then
      self.iStopVOIdx = self:_PlayVO(self.tStopFollowVO, self.iStopVOIdx)
    end
  end
  self.bVOOverride = nil
  if self._fCallback then
    if type(self._tCallbackData) == "table" then
      local tCopy = {
        unpack(self._tCallbackData)
      }
      table.insert(tCopy, uGuid)
      table.insert(tCopy, bEnable)
      self._fCallback(unpack(tCopy))
    else
      self._fCallback(uGuid, bEnable)
    end
  end
end

function _ToggleContextAction(self, bEnable)
  local vActor = self._vActor
  local uGuid = self._GetActorGuid(vActor)
  self:_RemoveContextAction()
  local sActionLabel = "[ContextAction.Follow]"
  local uCharacterGuid = 0
  if bEnable then
    sActionLabel = "[ContextAction.Stay]"
    uCharacterGuid = self._GetActorGuid(self._vObjectToFollow) or 0
  end
  local bSuccess = Pg.AddContextAction(uGuid, sActionLabel, 2, 0, 200, 0, 2, uCharacterGuid)
  ASSERT(bSuccess)
  Event.Delete(self._tEvents.uActionEvent)
  self._tEvents.uActionEvent = Event.Create(Event.ContextAction, {uCharacterGuid, uGuid}, self._Follow, {
    self,
    not bEnable
  })
end

function _RemoveContextAction(self)
  local vActor = self._vActor
  local uGuid = self._GetActorGuid(vActor)
  Pg.RemoveContextAction(uGuid)
end

function _GetActorGuid(vActor)
  local uGuid
  local sType = type(vActor)
  if sType == "string" then
    uGuid = Pg.GetGuidByName(vActor)
  elseif sType == "userdata" then
    uGuid = vActor
  end
  return uGuid
end

function _OnFollowerCanceled(self, uGuid, sReason)
  Debug.Printf("---=--- ", " uguid: ", uGuid, " reason: ", sReason)
  if sReason == "targettoofar" then
    self:_OnFollowerLost()
  elseif sReason == "targethostile" then
    self:_OnFollowerHostile()
  elseif sReason == "targetdead" then
    return
  end
  self:_Follow(false)
end

function _OnFollowerLost(self)
  self.iLostVOIdx = self:_PlayVO(self.tLostVO, self.iLostVOIdx)
  self.bVOOverride = true
  local uGuid = self._GetActorGuid(self._vActor)
  self._tEvents.eCloseEnough = Event.Create(Event.ObjectProximity, {
    uGuid,
    self._vObjectToFollow,
    "<",
    15,
    false,
    false
  }, _OnFollowerFound, {self})
end

function _OnFollowerFound(self)
  self.iFoundVOIdx = self:_PlayVO(self.tFoundVO, self.iFoundVOIdx)
  self.bVOOverride = true
  self:_Follow(true, self._vObjectToFollow)
end

function _OnFollowerHostile(self)
  self.iHostileVOIdx = self:_PlayVO(self.tHostileVO, self.iHostileVOIdx)
  self.bVOOverride = true
end

function _PlayVO(self, tTable, iIndex)
  if type(tTable) == "table" then
    MrxVoSequence.Start({
      {
        Human.DoAction,
        {
          self._vActor,
          "SpeakGestureUB"
        }
      },
      {
        tTable[iIndex],
        self._vActor
      },
      {
        Human.DoAction,
        {
          self._vActor,
          "ExitAction"
        }
      }
    }, false, MrxVoSequence.knPriorityFreeplay)
    iIndex = iIndex + 1
    if iIndex > table.getn(tTable) then
      iIndex = 1
    end
  end
  return iIndex
end

function _TransitEvalFn(tData)
  return true
end

function _OnTransitStart(self, tData)
  local res = Vehicle.Enter(tData[1], self._vActor, "p", true, false)
  Debug.Printf(" ooo ENTERING ", self._vActor, " ", tData[1], " ", res)
  if Vehicle.GetFromRider(self._vActor) == tData[1] then
    self._tEvents.eTransit = Event.Create(Event.ScriptEvent, {"transitEnd", _TransitEvalFn}, _OnTransitEnd, {self})
  else
    Debug.Printf("MrxFollow: transit vehicle full: ", tData[1], " ", self._vActor)
    self:_ToggleFollowingBehavior(false)
  end
end

function _OnTransitEnd(self, tData)
  local res = Vehicle.Exit(tData[1], self._vActor, false)
  self._tEvents.eTransit = Event.Create(Event.ScriptEvent, {
    "transitStart",
    _TransitEvalFn
  }, _OnTransitStart, {self})
  Debug.Printf("--0-- TRANSIT END: ", res, " event: ", self._tEvents.eTransit, " guid: ", tData[1])
end
