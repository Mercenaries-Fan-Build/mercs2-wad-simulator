import("MrxUtil")
import("MrxGuiInterface")

function Create(mModule, self)
  self = self or {}
  setmetatable(self, {__index = mModule})
  self.nStartTime = MrxUtil.SetDefault(self.nStartTime, 30)
  self.nStopTime = MrxUtil.SetDefault(self.nStopTime, 0)
  self.nStep = MrxUtil.SetDefault(self.nStep, 1)
  self.bUseTenths = MrxUtil.SetDefault(self.bUseTenths, false)
  self.nWarning = MrxUtil.SetDefault(self.nWarning, 5)
  self.iTray = MrxUtil.SetDefault(self.iTray, 1)
  self.bPlaySounds = MrxUtil.SetDefault(self.bPlaySounds, true)
  return self
end

function Start(self)
  self._iCurrentTime = self.nStartTime
  if self.nStartTime > self.nStopTime then
    self._bCountdown = true
  end
  self.Display(self)
  self._TimerEvent = Event.CreatePersistent(Event.TimerRelative, {
    self.nStep
  }, self._Update, {self})
  if self.bPlaySounds then
    Sound.CueSound(0, "ui_HUD_Timer_Start")
  end
end

function Display(self)
  local displayText
  displayText = Junk.FormatTime(self._iCurrentTime, self.bUseTenths)
  if self._bCountdown then
    if self._iCurrentTime <= self.nWarning then
      displayText = "[red]" .. displayText
    end
  elseif self._iCurrentTime >= self.nWarning then
    displayText = "[red]" .. displayText
  end
  if self.sLabel then
    displayText = self.sLabel .. " " .. displayText
  end
  local bRemainder
  if self._iCurrentTime - math.floor(self._iCurrentTime) >= 0.1 then
    bRemainder = true
  end
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = self.iTray,
    sText = displayText,
    bDontNetSync = bRemainder
  })
end

function Pause(self)
  Event.Delete(self._TimerEvent)
end

function Resume(self)
  self._TimerEvent = Event.CreatePersistent(Event.TimerRelative, {
    self.nStep
  }, self._Update, {self})
end

function AddTime(self, iTime)
  if iTime then
    self._iCurrentTime = self._iCurrentTime + iTime
  end
  self.Display(self)
end

function Stop(self)
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = self.iTray,
    sText = " "
  })
  Event.Delete(self._TimerEvent)
  self = nil
end

function _Update(self)
  local nPrevTime = self._iCurrentTime
  local nIncrementAlert = 60
  local nPrevStep, nCurStep
  local nRemainingTime = math.abs(self.nStopTime - self._iCurrentTime)
  if nRemainingTime < 10 then
    nIncrementAlert = 1
  elseif nRemainingTime < 60 then
    nIncrementAlert = 10
  end
  if self._bCountdown then
    nPrevStep = math.floor(self._iCurrentTime / nIncrementAlert)
    self._iCurrentTime = self._iCurrentTime - self.nStep
    nCurStep = math.floor(self._iCurrentTime / nIncrementAlert)
    if self._iCurrentTime < self.nStopTime then
      self._iCurrentTime = self.nStopTime
    end
  else
    nPrevStep = math.ceil(self._iCurrentTime / nIncrementAlert)
    self._iCurrentTime = self._iCurrentTime + self.nStep
    nCurStep = math.ceil(self._iCurrentTime / nIncrementAlert)
    if self._iCurrentTime > self.nStopTime then
      self._iCurrentTime = self.nStopTime
    end
  end
  if nPrevStep ~= nCurStep and self.bPlaySounds then
    Sound.CueSound(0, "ui_HUD_Timer_Increment")
  end
  if self._bCountdown then
    if nPrevTime > self.nWarning and self._iCurrentTime <= self.nWarning then
      if self.bPlaySounds then
        Sound.CueSound(0, "ui_HUD_Timer_Alert")
      end
      _CallCallbacks(self.tWarnCallbacks)
    end
  elseif nPrevTime < self.nWarning and self._iCurrentTime >= self.nWarning then
    if self.bPlaySounds then
      Sound.CueSound(0, "ui_HUD_Timer_Alert")
    end
    _CallCallbacks(self.tWarnCallbacks)
  end
  self.Display(self)
  if self._iCurrentTime == self.nStopTime then
    _CallCallbacks(self.tDoneCallbacks)
    self.Stop(self)
    if self.bPlaySounds then
      Sound.CueSound(0, "ui_HUD_Timer_End")
    end
  end
end

function GetTime(self)
  return self._iCurrentTime
end

function SetTime(self, iNewTime)
  if iNewTime then
    self._iCurrentTime = iNewTime
  end
end

function _CallCallbacks(t)
  if t then
    for _, tCallback in ipairs(t) do
      MrxUtil.CallWithOptionalArgs(tCallback[1], tCallback[2])
    end
  end
end
