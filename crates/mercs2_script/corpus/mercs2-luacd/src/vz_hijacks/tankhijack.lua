import("MrxGui")
import("MrxGuiHudActionHijack")
local auto_hijack = false
local hatch_pressed_needed = 5

function StartHijack(hijackerObject, hijackeeObject, seatObject, vehicleObject)
  _THIS:Create(nil, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  return true
end

function Create(mModule, self, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  self = self or {}
  self._hijacker = hijackerObject
  self._hijackee = hijackeeObject
  self._seat = seatObject
  self._vehicle = vehicleObject
  setmetatable(self, {__index = mModule})
  Object.DisablePhysics(hijackerObject)
  self._hijackerPlayer = Object.IsPlayerControlled(self._hijacker)
  if self._hijackerPlayer and not auto_hijack then
    self._buttonPressed = false
    self._buttonTimer = Event.Create(Event.TimerRelative, {2}, ButtonStart, {self})
  else
    self._buttonPressed = true
  end
  Vehicle.EnableTurret(self._vehicle, "main_turret", false, "all")
  Vehicle.SetTurretPitch(self._vehicle, "main_turret", 0)
  Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerA")
  self._actionTimer = Event.Create(Event.HumanActionComplete, {
    self._hijacker
  }, CompleteQTESequence, {self})
  Ai.Enable(self._hijackee, false)
  Vehicle.StartTankHijackMotion(self._vehicle)
  return self
end

function ButtonStart(self)
  self._eventButton = Event.Create(Event.Minigame, {
    self._hijackerPlayer,
    2,
    "hold",
    Controller.RPad_Up
  }, ButtonPressed, {self})
  self._buttonTimer = Event.Create(Event.TimerRelative, {3}, ButtonTimeout, {self})
  MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, true)
  MrxGuiHudActionHijack.SetDisplayButton(self._hijackerPlayer, Controller.RPad_Up)
  MrxGuiHudActionHijack.SetDisplayMashAnimation(self._hijackerPlayer, false)
end

function ButtonTimeout(self)
  if self._buttonPressed == false then
    MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, false)
    Event.Delete(self._eventButton)
    Event.Delete(self._actionTimer)
    Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerFailA")
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

function ButtonPressed(a, b, c)
  Debug.Printf("OnMinigameStatus: called " .. ", " .. tostring(b) .. ", " .. tostring(c))
end

function Getup(self)
  Human.SetState(self._hijacker, "Upright", "Idle")
end

function CompleteQTESequence(self)
  if auto_hijack then
    self._numPresses = hatch_pressed_needed
  else
    self._numPresses = 0
  end
  Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerBLoop")
  MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, true)
  MrxGuiHudActionHijack.SetDisplayButton(self._hijackerPlayer, MrxGui.Joystick.BUTTON_PAD2_R)
  MrxGuiHudActionHijack.SetDisplayMashAnimation(self._hijackerPlayer, true)
  self._eventButton = Event.CreatePersistent(Event.Minigame, {
    self._hijackerPlayer,
    "BUTTON_PAD2_RIGHT"
  }, CountPresses, {self})
  self._eventTimer = Event.Create(Event.TimerRelative, {3}, TugOfWarTimeout, {self})
end

function CountPresses(self, value)
  if value == 1 then
    self._numPresses = self._numPresses + 1
  end
end

function TugOfWarTimeout(self)
  MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, false)
  Event.Delete(self._eventButton)
  if self._numPresses >= hatch_pressed_needed then
    self:StartGrenadeTakeover()
  else
    Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerFailB")
    self._actionTimer = Event.Create(Event.HumanActionComplete, {
      self._hijacker
    }, RagdollFailure, {self})
  end
end

function StartGrenadeTakeover(self)
  Human.SetState(self._hijacker, "InVehicle", "ActionHijackHijackerC")
  self._explosionTimer = Event.Create(Event.TimerRelative, {2.6}, function(self)
    local x, y, z = Object.GetHardpointPosition(self._hijacker, "Bone_Attach_RHand")
    Pg.SpawnTankHijackExplosion(x, y, z, 0, 1, 0, 0)
  end, {self})
  self._actionTimer = Event.Create(Event.HumanActionComplete, {
    self._hijacker
  }, Complete, {self})
end

function Complete(self)
  Vehicle.HijackComplete(self._hijacker)
  Object.Remove(self._hijackee)
end
