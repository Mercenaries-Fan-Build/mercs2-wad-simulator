inherit("MrxTaskObjective")
import("MrxSupportData")
import("MrxFollow")
import("MrxUtil")
import("MrxGui")

function Activated(self)
  MrxTaskObjective.Activated(self)
  local tConfig = self:GetConfig()
  tConfig.fDist = MrxUtil.SetDefault(tConfig.fDist, 40)
  tConfig.bStop = MrxUtil.SetDefault(tConfig.bStop, false)
  tConfig.bXZOnly = MrxUtil.SetDefault(tConfig.bXZOnly, false)
  tConfig.bHumansFollow = MrxUtil.SetDefault(tConfig.bHumansFollow, true)
  local tObjects = ObjectFilter.GetObjects(self._uTgtObjFilter, false)
  local uGuid = tObjects[1]
  if tConfig.oFollower then
    self.oFollower = tConfig.oFollower
  elseif tConfig.bHumansFollow then
    local tFollowerConfig = {
      _vActor = uGuid,
      _vObjectToFollow = tConfig.uStartAttachedToPlayer,
      _fCallback = _OnAttachment,
      _tCallbackData = {self, "follow"},
      tStartFollowVO = tConfig.tStartFollowVO,
      tStopFollowVO = tConfig.tStopFollowVO,
      tLostVO = tConfig.tLostVO,
      tFoundVO = tConfig.tFoundVO
    }
    local oFollower = MrxFollow:Create(tFollowerConfig)
    oFollower:Activate(true, tConfig.uStartAttachedToPlayer ~= nil)
    self.oFollower = oFollower
  end
  local uFilter = ObjectFilter.Create()
  CheckForHeli(self, uGuid)
  self:_CreateEvent(Event.ObjectDeath, {
    self._uTgtObjFilter
  }, TargetDestroyed, {self})
  MrxSupportData.AddFreebie("Extraction_AL")
  _evClientJoined = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, SendPlayerJoinEvents)
end

function SendPlayerJoinEvents()
  if not Net.IsServer() then
    return
  end
  MrxSupportData.AddFreebie("Extraction_AL", nil, Player.GetSecondaryPlayer())
end

function CheckForHeli(self, uGuid)
  local uExtractFilter = ObjectFilter.Create()
  ObjectFilter.SetFilter(uExtractFilter, "Allied && Helicopter")
  eHeliClose = self:_CreateEvent(Event.ObjectProximity, {
    uExtractFilter,
    uGuid,
    "<",
    40,
    false,
    false
  }, TargetStopsForHeli, {self, uGuid})
end

function TargetDestroyed(self, uGuid)
  self:CancelPart(uGuid)
end

function TargetStopsForHeli(self, uGuid, tHeliExt)
  local uDriver = Vehicle.GetDriver(tHeliExt[1])
  if uDriver and Object.HasLabel(uDriver, "Allied") then
    Debug.Printf("The Extaction heli arrived")
    if self.oFollower then
      self.oFollower:Activate(false)
    end
    local uInVeh = Vehicle.GetFromRider(uGuid)
    if uInVeh then
      Ai.Goal({
        AIGuid = uGuid,
        Goal = "Exit",
        Priority = "hiPri",
        Force = true,
        Callback = TargetRunsForHeli,
        CallbackData = {
          self,
          uGuid,
          tHeliExt,
          "Target"
        }
      })
    end
    self:_CreateEvent(Event.TimerRelative, {3}, TargetRunsForHeli, {
      self,
      uGuid,
      tHeliExt,
      "Target"
    })
    uHeliHurt = self:_CreateEvent(Event.ObjectHealth, {
      tHeliExt[1],
      "<",
      Object.GetHealth(tHeliExt[1]) - 25
    }, AbortExtract, {self, uGuid})
    eHeliFailsafe = self:_CreateEvent(Event.TimerRelative, {50}, AbortExtract, {self, uGuid})
    eHeliFar = self:_CreateEvent(Event.ObjectProximity, {
      tHeliExt[1],
      uGuid,
      ">",
      70,
      false,
      false
    }, AbortExtract, {self, uGuid})
    self:_CreateEvent(Event.ObjectInSeat, {
      uGuid,
      tHeliExt[1],
      "a",
      "e"
    }, TargetIn, {
      self,
      uGuid,
      tHeliExt,
      uGuid,
      1
    })
  else
    CheckForHeli(self, uGuid)
  end
end

function TargetRunsForHeli(self, uGuid, tHeliExt)
  Debug.Printf("Target running for helicopter")
  tEnterGoal = {
    AIGuid = uGuid,
    Goal = "Enter",
    Target = tHeliExt[1],
    Priority = "hiPri",
    Force = true,
    Callback = CheckEnter,
    CallbackData = {self, tHeliExt}
  }
  eAIenter1 = self:_CreateEvent(Event.TimerRelative, {2}, Ai.Goal, {tEnterGoal})
end

function CheckEnter(self, tHeliExt, uGuid, nState)
  if nState == 0 then
    eAIenter2 = self:_CreateEvent(Event.TimerRelative, {1}, TargetRunsForHeli, {
      self,
      uGuid,
      tHeliExt
    })
  elseif eHeliFailsafe then
    Event.Delete(eHeliFailsafe)
  end
end

function AbortExtract(self, uGuid)
  Debug.Printf("Extraction heli was destroyed!")
  if eHeliFar then
    Event.Delete(eHeliFar)
  end
  if eHeliFailsafe then
    Event.Delete(eHeliFailsafe)
  end
  if eHeliClose then
    Event.Delete(eHeliClose)
  end
  if eAIenter1 then
    Event.Delete(eAIenter1)
  end
  if eAIenter2 then
    Event.Delete(eAIenter2)
  end
  Ai.Goal({
    AIGuid = uGuid,
    Target = Player.GetLocalCharacter(),
    Goal = "Face",
    Position = true,
    Priority = "hiPri"
  })
  self:_CreateEvent(Event.TimerRelative, {2}, ResetPrisoner, {self, uGuid})
end

function ResetPrisoner(self, uGuid)
  Ai.RemoveGoal({AIGuid = uGuid, Handle = 0})
  if self.oFollower then
    self.oFollower:Activate(true)
  end
  CheckForHeli(self, uGuid)
end

function TargetIn(self, uGuid, tHeliExt, Guid, State)
  Debug.Printf("Target in helicopter")
  self:_CreateEvent(Event.TimerRelative, {4}, self.CompletePart, {self, uGuid})
end

function Cleanup(self)
  MrxSupportData.RemoveFreebie("Extraction_AL")
  MrxTaskObjective.Cleanup(self)
end
