import("MrxTimer")
import("MrxUtil")
import("MrxVoSequence")

function Create(srcObj, tConfig)
  if tConfig == nil then
    return
  end
  local self = {}
  setmetatable(self, srcObj)
  srcObj.__index = srcObj
  self._tConfig = tConfig
  self.tEvents = {}
  local uRgn
  if type(tConfig.sRegionName) == "string" then
    uRgn = Pg.GetGuidByName(tConfig.sRegionName)
  else
    uRgn = tConfig.sRegionName
  end
  if uRgn then
    self.uRgn = uRgn
    self.tEvents.eOutside = Event.Create(Event.Boundary, {
      Player.GetAnyCharacter(),
      self.uRgn,
      "exit",
      false
    }, _OutsideBoundary, {self})
  else
    if type(tConfig.sPoint) == "string" then
      self.uPoint = Pg.GetGuidByName(tConfig.sPoint)
    else
      self.uPoint = tConfig.sPoint
    end
    self.fRadius = tConfig.fRadius
    self.tEvents.eOutside = Event.Create(Event.ObjectProximity, {
      Player.GetAnyCharacter(),
      self.uPoint,
      ">",
      self.fRadius,
      false,
      true
    }, _OutsideRange, {self})
  end
  self.fCallback = tConfig.fCallback
  self.tCallbackData = tConfig.tCallbackData
  self.fWarnTime = MrxUtil.SetDefault(tConfig.fWarnTime, 15)
  self.fFailTime = MrxUtil.SetDefault(tConfig.fFailTime, 30)
  self.iTray = MrxUtil.SetDefault(tConfig.iTray, 3)
  return self
end

function Cancel(self)
  for i, e in pairs(self.tEvents) do
    Event.Delete(e)
    self.tEvents[i] = nil
  end
  if self.oTimer then
    self.oTimer:Stop()
    self.oTimer = nil
  end
end

function GetRegion(self)
  return self.uRgn
end

function _OutsideBoundary(self, uCharacter)
  local uPrimary = Player.GetPrimaryCharacter()
  local uSecondary = Player.GetSecondaryCharacter()
  if uSecondary == uCharacter and uPrimary and Object.GetDistanceFrom(uPrimary, uCharacter) > Pg.GetTetherDiameterStart() then
    self.tEvents.eOutside = Event.Create(Event.Boundary, {
      Player.GetAnyCharacter(),
      self.uRgn,
      "exit",
      false
    }, _OutsideBoundary, {self})
    return
  end
  self:_CallCallback("exit")
  self.tEvents.eReturn = Event.Create(Event.Boundary, {
    uCharacter,
    self.uRgn,
    "enter",
    false
  }, _InsideBoundary, {self})
  if self._tConfig.tExitVOs then
    local line = MrxUtil.GetRandomTableElement(self._tConfig.tExitVOs)
    MrxVoSequence.Start({
      line,
      {
        _StartTimer,
        {self}
      }
    })
  else
    self:_StartTimer()
  end
end

function _InsideBoundary(self, uCharacter)
  local tPlayers = Player.GetAllPlayers()
  for i, p in pairs(tPlayers) do
    local char = Player.GetCharacter(p)
    if char and char ~= uCharacter and not Object.InsideBoundary(char, self.uRgn) then
      self.tEvents.eReturn = Event.Create(Event.Boundary, {
        char,
        self.uRgn,
        "enter",
        false
      }, _InsideBoundary, {self})
      return
    end
  end
  if self.oTimer then
    self.oTimer:Stop()
    self.oTimer = nil
  end
  self.tEvents.eOutside = Event.Create(Event.Boundary, {
    Player.GetAnyCharacter(),
    self.uRgn,
    "exit",
    false
  }, _OutsideBoundary, {self})
  self:_CallCallback("return")
end

function _OutsideRange(self, uCharacter)
  local uPrimary = Player.GetPrimaryCharacter()
  local uSecondary = Player.GetSecondaryCharacter()
  if uSecondary == uCharacter and uPrimary and Object.GetDistanceFrom(uPrimary, uCharacter) > Pg.GetTetherDiameterStart() then
    self.tEvents.eOutside = Event.Create(Event.ObjectProximity, {
      Player.GetAnyCharacter(),
      self.uPoint,
      ">",
      self.fRadius,
      false,
      true
    }, _OutsideRange, {self})
    return
  end
  self:_CallCallback("exit")
  self.tEvents.eReturn = Event.Create(Event.ObjectProximity, {
    uCharacter,
    self.uPoint,
    "<=",
    self.fRadius - 10,
    false,
    true
  }, _InsideRange, {self})
  if self._tConfig.tExitVOs then
    local line = MrxUtil.GetRandomTableElement(self._tConfig.tExitVOs)
    MrxVoSequence.Start({
      line,
      {
        _StartTimer,
        {self}
      }
    })
  else
    self:_StartTimer()
  end
end

function _InsideRange(self, uCharacter)
  local tPlayers = Player.GetAllPlayers()
  for i, p in pairs(tPlayers) do
    local char = Player.GetCharacter(p)
    if char and char ~= uCharacter and Object.GetDistanceFrom(char, self.uPoint, true) > self.fRadius then
      self.tEvents.eReturn = Event.Create(Event.ObjectProximity, {
        uCharacter,
        self.uPoint,
        "<=",
        self.fRadius - 10,
        false,
        true
      }, _InsideRange, {self})
      return
    end
  end
  if self.oTimer then
    self.oTimer:Stop()
    self.oTimer = nil
  end
  self.tEvents.eOutside = Event.Create(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    self.uPoint,
    ">",
    self.fRadius,
    false,
    true
  }, _OutsideRange, {self})
  if self._tConfig.tReturnVOs then
    local line = MrxUtil.GetRandomTableElement(self._tConfig.tReturnVOs)
    MrxVoSequence.Start({line})
  end
  self:_CallCallback("return")
end

function _StartTimer(self)
  local tP = Player.GetAllPlayers()
  for i, p in pairs(tP) do
    if Object.GetDistanceFrom(Player.GetCharacter(p), self.uPoint) < self.fRadius - 10 then
      return
    end
  end
  self.oTimer = MrxTimer:Create({
    nStartTime = self.fFailTime,
    nWarning = self.fWarnTime,
    iTray = self.iTray,
    sLabel = self._tConfig.sLabel,
    tDoneCallbacks = {
      {
        _FailTimeExpired,
        {self}
      }
    },
    tWarnCallbacks = {
      {
        _WarnTimeExpired,
        {self}
      }
    }
  })
  self.oTimer:Start()
end

function _WarnTimeExpired(self)
  if self._tConfig.tWarnVOs then
    local line = MrxUtil.GetRandomTableElement(self._tConfig.tWarnVOs)
    MrxVoSequence.Start({line})
  end
  self:_CallCallback("warning")
end

function _FailTimeExpired(self)
  self:_CallCallback("fail")
  Event.Delete(self.tEvents.eReturn)
  self.tEvents.eReturn = nil
  self.oTimer:Stop()
  self.oTimer = nil
end

function _CallCallback(self, sStatus)
  if not self.fCallback then
    return
  end
  if self.tCallbackData then
    self.fCallback(self, sStatus, unpack(self.tCallbackData))
  else
    self.fCallback(self, sStatus)
  end
end
