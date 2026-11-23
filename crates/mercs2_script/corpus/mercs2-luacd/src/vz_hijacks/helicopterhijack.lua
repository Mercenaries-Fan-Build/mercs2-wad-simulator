import("MrxGui")
import("MrxGuiHudActionHijack")
local auto_hijack = 1
local hatch_pressed_needed = 5
local hijack_times = {
  1.9,
  2.83,
  0.66,
  0.46,
  0.4
}

function StartHijack(hijackerObject, hijackeeObject, seatObject, vehicleObject)
  if Vehicle.IsFlying(vehicleObject) then
    _THIS:Create(nil, hijackerObject, hijackeeObject, seatObject, vehicleObject)
    return true
  end
  return false
end

function Create(mModule, self, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  self = self or {}
  self._hijacker = hijackerObject
  self._hijackee = hijackeeObject
  self._seat = seatObject
  self._vehicle = vehicleObject
  setmetatable(self, {__index = mModule})
  self._hijackerPlayer = Object.IsPlayerControlled(self._hijacker)
  if self._hijackerPlayer and not auto_hijack then
    self._buttonPressed = false
    self._buttonTimer = Event.Create(Event.TimerRelative, {0.9}, ButtonStart, {self})
  else
    self._buttonPressed = true
  end
  Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerA")
  Human.SetState(self._hijackee, "InVehicle", "ActionHijackHijackeeA")
  self._actionTimer = Event.Create(Event.HumanActionComplete, {
    self._hijacker
  }, PullOutPilot, {self})
  Ai.Enable(self._hijackee, false)
  Vehicle.SpinHeli(self._vehicle)
  return self
end

function ButtonStart(self)
  self._eventButton = Event.Create(Event.Minigame, {
    self._hijackerPlayer,
    "BUTTON_PAD2_UP"
  }, ButtonPressed, {self})
  self._buttonTimer = Event.Create(Event.TimerRelative, {1}, ButtonTimeout, {self})
  MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, true)
  MrxGuiHudActionHijack.SetDisplayButton(self._hijackerPlayer, MrxGui.Joystick.BUTTON_PAD2_U)
  MrxGuiHudActionHijack.SetDisplayMashAnimation(self._hijackerPlayer, false)
end

function ButtonTimeout(self)
  if self._buttonPressed == false then
    MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, false)
    Event.Delete(self._eventButton)
    Event.Delete(self._actionTimer)
    Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerFailA")
    Human.SetState(self._hijackee, "InVehicle", "ActionHijackHijackeeFailA")
    self._actionTimer = Event.Create(Event.HumanActionComplete, {
      self._hijacker
    }, RagdollFailure, {self})
  end
end

function RagdollFailure(self)
  Human.SetState(self._hijacker, "Upright", "MeleeKnockedDown")
  Vehicle.HijackAbort(self._hijacker)
  Ai.Enable(self._hijackee, true)
  self._eventTimer = Event.Create(Event.TimerRelative, {2}, Getup, {self})
end

function ButtonPressed(self, value)
  if value == 1 then
    self._buttonPressed = true
    MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, false)
  else
    self._eventButton = Event.Create(Event.Minigame, {
      self._hijackerPlayer,
      "BUTTON_PAD2_UP"
    }, ButtonPressed, {self})
  end
end

function Getup(self)
  Human.SetState(self._hijacker, "Upright", "Idle")
end

function PullOutPilot(self)
  if auto_hijack then
    self._numPresses = hatch_pressed_needed
  else
    self._numPresses = 0
  end
  Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerB")
  Human.SetState(self._hijackee, "InVehicle", "ActionHijackHijackeeB")
  MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, true)
  MrxGuiHudActionHijack.SetDisplayButton(self._hijackerPlayer, MrxGui.Joystick.BUTTON_PAD2_R)
  MrxGuiHudActionHijack.SetDisplayMashAnimation(self._hijackerPlayer, true)
  self._eventButton = Event.CreatePersistent(Event.Minigame, {
    self._hijackerPlayer,
    "BUTTON_PAD2_RIGHT"
  }, CountPresses, {self})
  self._actionTimer = Event.Create(Event.HumanActionComplete, {
    self._hijacker
  }, function(self)
    Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerC")
    Human.SetState(self._hijackee, "InVehicle", "ActionHijackHijackeeC")
    self._actionTimer = Event.Create(Event.HumanActionComplete, {
      self._hijacker
    }, function(self)
      Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerD")
      Human.SetState(self._hijackee, "InVehicle", "ActionHijackHijackeeD")
      self._actionTimer = Event.Create(Event.HumanActionComplete, {
        self._hijacker
      }, PunchesDone, {self})
    end, {self})
  end, {self})
end

function CountPresses(self, value)
  if value == 1 then
    self._numPresses = self._numPresses + 1
  end
end

function PunchesDone(self)
  MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, false)
  Event.Delete(self._eventButton)
  if self._numPresses >= hatch_pressed_needed then
    self:ClimbInToCockpit()
  else
    Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerFailB")
    Human.SetState(self._hijackee, "InVehicle", "ActionHijackHijackeeFailB")
    self._actionTimer = Event.Create(Event.HumanActionComplete, {
      self._hijacker
    }, RagdollFailure, {self})
  end
end

function ClimbInToCockpit(self)
  Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerE")
  Human.SetState(self._hijackee, "InVehicle", "ActionHijackHijackeeE")
  self._actionTimer = Event.Create(Event.HumanActionComplete, {
    self._hijacker
  }, Complete, {self})
end

function Complete(self)
  Vehicle.HijackComplete(self._hijacker)
  Object.Remove(self._hijackee)
end
