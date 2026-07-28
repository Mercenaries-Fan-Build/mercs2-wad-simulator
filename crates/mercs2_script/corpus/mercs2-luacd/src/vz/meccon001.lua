inherit("MrxTaskContract")
import("MrxUtil")
import("MrxVoSequence")
import("MrxApcDrop")
import("DangerousBuilding")
import("MrxTimer")
import("MrxMusic")
import("MrxTutorialManager")
import("MrxGuiInterface")
import("MrxPlayState")

function LoadAssets(self, tSaveData)
  local tLayersToAdd = {
    "vz_state_gua_upperclass_pristine",
    "Vz_State_MecJob",
    "VZ_State_MecCon001"
  }
  MrxLayerManager.Add(tLayersToAdd, self._SetupVehicles1, {self})
end

function _SetupVehicles1(self)
  if Net.DoneReloadingLayers then
    Net.DoneReloadingLayers()
  end
  if self:_GetFlag("race_cp") ~= nil then
    giAttempts = self:_GetFlag("race_cp") + 1
    self.uCar = MrxUtil.SpawnObject("Monster Truck", "mc001.car.respawn")
    if 1 < Player.GetCurrentPlayers() then
      self.uBike = MrxUtil.SpawnObject("Offroad Motorcycle (GR)", "mc001.bike.respawn")
    end
    if Object.IsHibernated(self.uCar) then
      self:_CreateEvent(Event.ObjectHibernation, {
        self.uCar,
        "awake"
      }, _SetupVehicles2, {self})
    else
      self:_SetupVehicles2()
    end
  else
    giAttempts = 1
    self.uCar = MrxUtil.SpawnObject("Monster Truck", "meccon_monster")
    if 1 < Player.GetCurrentPlayers() then
      MrxUtil.SpawnObject("Offroad Motorcycle (GR)", "mc001.bike.spawn")
    end
    self:AssetsLoaded()
  end
end

function _SetupVehicles2(self)
  if Player.IsCoopMultiplayer() then
    local uPrimaryPlayerVeh, uSecondaryPlayerVeh
    if self:_GetFlag("race_P2") then
      uPrimaryPlayerVeh = self.uBike
      uSecondaryPlayerVeh = self.uCar
    else
      uPrimaryPlayerVeh = self.uCar
      uSecondaryPlayerVeh = self.uBike
    end
    Vehicle.Enter(uPrimaryPlayerVeh, Player.GetPrimaryCharacter(), "d", true, false)
    Net.SendCustomEvent("MecCon001", NETEVENT_ENTERVEHICLE, {uSecondaryPlayerVeh}, true)
    self._tEvents.eMPwait = Event.Create(Event.ScriptEvent, {
      "mpPlayerLeft",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, _MyOnPlayerLeft, {self})
  else
    Vehicle.Enter(self.uCar, Player.GetPrimaryCharacter(), "d", true)
  end
  self._tEvents.eVehicleSetup = Event.Create(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    self.uCar,
    "d",
    "e"
  }, AssetsLoaded, {self})
end

function _MyOnPlayerLeft(self, tData)
  Debug.Printf(" =*= SECONDARY PLAYER LEFT ON RETRY ")
  if self._tEvents.eVehicleSetup and self:_GetFlag("race_P2") then
    Debug.Printf(" =*= Secondary player, who was driver of car, has left ")
    Event.Delete(self._tEvents.eVehicleSetup)
    Vehicle.Enter(self.uCar, Player.GetPrimaryCharacter(), "d", true)
    self._tEvents.eVehicleSetup = Event.Create(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      self.uCar,
      "d",
      "e"
    }, AssetsLoaded, {self})
  end
end

function Activated(self)
  MrxTaskContract.Activated(self)
  self._tEvents.eVehicleSetup = nil
  self.inRegion = Pg.GetGuidByName("mechanicHQ.rgn.inside")
  self.outRegion = Pg.GetGuidByName("mechanicHQ.rgn.outside")
  self.garage = Pg.GetGuidByName("mechanicHQ")
  self:_CreateEvent(Event.ObjectDeath, {
    self.uCar
  }, MonsterTruckDestroyed, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    self.garage
  }, GarageDestroyed, {self})
  if self:_GetFlag("race_cp") ~= nil then
    Object.SetInvincible(self.uCar, false)
    if self.uBike then
      Object.SetInvincible(self.uBike, false)
    end
    self:ObjGoToDestination()
  else
    local tVO = {
      "Eva-In-Mission-Contract-Mech01-22",
      0,
      {
        mattias = "Mattias-In-Mission-Contract-Mech01-23",
        jennifer = "Jennifer-In-Mission-Contract-Mech01-24",
        chris = "Chris-In-Mission-Contract-Mech01-25"
      },
      0.2,
      "Eva-In-Mission-Contract-Mech01-26",
      {
        ObjGetInVehicle,
        {self}
      }
    }
    if Player.GetCurrentPlayers() > 1 then
      table.insert(tVO, table.getn(tVO), "Eva-In-Mission-Contract-Mech01-72")
    end
    MrxVoSequence.Start(tVO)
  end
  MrxMusic.PlaySpecialMusic("mu_mission_meccon001_01")
  local uGun = StringToGuid("0xd047d")
  self:_CreateEvent(Event.ObjectHibernation, {uGun, "awake"}, Object.Kill, {uGun})
end

function Cleanup(self)
  MrxTutorialManager.HideMessage()
  Net.SendCustomEvent("MecCon001", NETEVENT_HIDETUT, {})
  self:TutorialCancel()
  if self.curAiGoal then
    Ai.RemoveGoal(self.curAiGoal)
    self.curAiGoal = nil
  end
  
  local function CleanupVehicle(uVeh)
    local tRiders = Vehicle.GetRiders(uVeh)
    for i, uRider in pairs(tRiders) do
      local uPlayer = Object.IsPlayerControlled(uRider)
      if uPlayer == nil or Player.IsLocal(uPlayer) then
        Vehicle.Exit(uVeh, uRider, true)
      end
    end
    Object.FadeOut(uVeh, 1, true)
  end
  
  CleanupVehicle(self.uCar)
  if self.uBike then
    CleanupVehicle(self.uBike)
  end
  local uGate = Pg.GetGuidByName("vz_mine_gate")
  Object.CloseGate(uGate)
  local tLayers = {
    "Vz_State_MecJob",
    "VZ_State_MecCon001"
  }
  for _, sLayer in pairs(tLayers) do
    MrxLayerManager.MarkForRemoval(sLayer)
  end
  MrxTaskContract.Cleanup(self)
end

function MissionComplete(self)
  self:_CreateEvent(Event.TimerRelative, {2}, self.Complete, {self})
end

function MonsterTruckDestroyed(self)
  self:_SetCancelMessage("[MecCon001.Terms.Cancel02]")
  MrxVoSequence.Start({
    "Eva-In-Mission-Contract-Mech01-47",
    {
      self.Cancel,
      {self}
    }
  })
end

function GarageDestroyed(self)
  self:_SetCancelMessage("[MecCon001.Terms.Cancel01]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Mech01-62",
    {
      self.Cancel,
      {self}
    }
  })
end

function CreateTutorialTrigger(self)
  self._tEvents.eJumpTutorialProx = Event.Create(Event.ObjectProximity, {
    self.uCar,
    Pg.GetGuidByName("mc001.loc.tutorialtrigger"),
    "<",
    25,
    false,
    false
  }, StartTutorial, {self})
end

function StartTutorial(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Mech01-35"
  })
  self.bShowTutorialTray = true
  self:SetupTutorialTray()
end

function SetupJumpTutorial(self)
  self:CreateTutorialTrigger()
  local uVehicle = self.uCar
  local uDriver = Vehicle.GetDriver(uVehicle)
  if uDriver and Object.IsPlayerControlled(uDriver) then
    self:JumpTutorial_InCar(uDriver, uVehicle)
  else
    self:JumpTutorial_OutCar(uDriver, uVehicle)
  end
end

function JumpTutorial_OutCar(self, uChar, uVehicle)
  Event.Delete(self._tEvents.eJumpTutorial)
  self._tEvents.eJumpTutorial = nil
  self._tEvents.eJumpTutorialSeat = Event.Create(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    uVehicle,
    "d",
    "ei"
  }, JumpTutorial_InCar, {self})
  MrxTutorialManager.HideMessage()
  Net.SendCustomEvent("MecCon001", NETEVENT_HIDETUT, {})
end

function JumpTutorial_InCar(self, uDriver, uVehicle)
  local uPlayer = Object.IsPlayerControlled(uDriver)
  self._tEvents.eJumpTutorial = Event.Create(Event.Button, {
    uPlayer,
    "rtrigger",
    "press",
    true
  }, TutorialComplete, {self})
  self._tEvents.eJumpTutorialSeat = Event.Create(Event.ObjectInSeat, {
    uDriver,
    uVehicle,
    "d",
    "xo"
  }, JumpTutorial_OutCar, {self})
  if self.bShowTutorialTray then
    self:SetupTutorialTray()
  end
end

function SetupTutorialTray(self)
  local uDriver = Vehicle.GetDriver(self.uCar)
  if uDriver then
    local uPlayer = Object.IsPlayerControlled(uDriver)
    if uPlayer == Player.GetPrimaryPlayer() then
      MrxTutorialManager.ShowMessage("[MecCon001.Objectives.buttonTray]")
    else
      Net.SendCustomEvent("MecCon001", NETEVENT_SHOWTUT, {})
    end
  end
end

function TutorialComplete(self)
  self:TutorialCancel()
  MrxVoSequence.Stop(true)
  MrxVoSequence.Start({
    "Eva-In-Mission-Contract-Mech01-36",
    0,
    {
      mattias = "Mattias-In-Mission-Contract-Mech01-37",
      jennifer = "Jennifer-In-Mission-Contract-Mech01-38",
      chris = "Chris-In-Mission-Contract-Mech01-39"
    },
    0.5,
    "Eva-In-Mission-Contract-Mech01-40"
  })
end

function TutorialCancel(self)
  self.bShowTutorialTray = nil
  if self._tEvents.eJumpTutorial then
    MrxTutorialManager.HideMessage()
    Net.SendCustomEvent("MecCon001", NETEVENT_HIDETUT, {})
    Event.Delete(self._tEvents.eJumpTutorial)
    self._tEvents.eJumpTutorial = nil
  end
  if self._tEvents.eJumpTutorialProx then
    Event.Delete(self._tEvents.eJumpTutorialProx)
    self._tEvents.eJumpTutorialProx = nil
  end
  if self._tEvents.eJumpTutorialSeat then
    Event.Delete(self._tEvents.eJumpTutorialSeat)
    self._tEvents.eJumpTutorialSeat = nil
  end
end

function ObjGetInVehicle(self)
  local uDriver = Vehicle.GetDriver(self.uCar)
  if uDriver and Object.IsPlayerControlled(uDriver) then
    MrxVoSequence.Stop(true)
    self:ObjDriveAroundBlock()
  else
    local oEnterObj = self:CreateChild({
      sName = "Enter car",
      sModuleName = "MrxTaskObjectiveEnterVehicle",
      sDspShortDesc = "[MecCon001.Objectives.enterVehicle]",
      vTgtInclude = self.uCar,
      nQuota = 1,
      fOnComplete = function()
        self:ObjDriveAroundBlock()
      end
    })
  end
end

function ObjDriveAroundBlock(self)
  self:_CreateEvent(Event.TimerRelative, {1}, MrxVoSequence.Start, {
    {
      "Eva-In-Mission-Contract-Mech01-27",
      1.5,
      {
        mattias = "Mattias-In-Mission-Contract-Mech01-29",
        jennifer = "Jennifer-In-Mission-Contract-Mech01-30",
        chris = "Chris-In-Mission-Contract-Mech01-31"
      },
      0,
      "Eva-In-Mission-Contract-Mech01-32",
      {
        SetupJumpTutorial,
        {self}
      }
    }
  })
  self:CreateChild({
    sName = "MecRace",
    sModuleName = "MrxTaskRace",
    sDspShortDesc = "[MecCon001.Objectives.driveAround]",
    vTgtInclude = self.uCar,
    tCourseLocs = {
      "mc001.race.001",
      "mc001.race.002",
      "mc001.race.003",
      "mc001.race.004",
      "mc001.race.001",
      "mc001.race.005"
    },
    fOnComplete = function()
      self:ObjGoToDestination()
    end,
    fOnCancel = function()
      if Object.IsAlive(self.uCar) then
        self:Cancel()
      end
    end
  })
end

function ObjGoToDestination(self)
  self:CreateChild({
    sName = "mc001 go to",
    sModuleName = "MrxTaskObjectiveDeliver",
    vDestLoc = "mc001.mines",
    fDist = 8,
    bXZOnly = true,
    vTgtInclude = self.uCar,
    bStop = false,
    bDetach = false,
    sDspShortDesc = "[MecCon001.Objectives.gotoMine]",
    fOnComplete = function()
      self:AtRaceStart()
    end,
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {
      "Eva-In-Mission-Contract-Mech01-33",
      "Eva-In-Mission-Contract-Mech01-34"
    }
  })
end

function AtRaceStart(self)
  self:TutorialCancel()
  self:_SetFlag("race_cp", giAttempts)
  local uChar = Vehicle.GetDriver(self.uCar)
  if uChar ~= nil and uChar == Player.GetSecondaryCharacter() then
    self:_SetFlag("race_P2")
  end
  _Checkpoint({
    "meccon001.respawn.p1",
    "meccon001.respawn.p2"
  })
  MrxVoSequence.Start({
    "Eva-In-Mission-Contract-Mech01-41"
  })
  self:StartRace()
end

function StartRace(self)
  local kGoalTime = 45
  if giAttempts > 6 then
    kGoalTime = 90
  elseif giAttempts > 3 then
    kGoalTime = 60
  end
  self:CreateChild({
    sName = "MecRace",
    sModuleName = "MrxTaskRace",
    vTgtInclude = self.uCar,
    tCourseLocs = {
      "mc001.race.019",
      "mc001.race.020",
      "mc001.race.020-1",
      "mc001.race.021",
      "mc001.race.022",
      "mc001.race.023",
      "mc001.race.024",
      "mc001.race.025",
      "mc001.race.026",
      "mc001.race.027",
      "mc001.race.028",
      "mc001.race.029",
      "mc001.race.030",
      "mc001.race.032",
      "mc001.race.034",
      "mc001.race.035",
      "mc001.race.035-1",
      "mc001.race.036",
      "mc001.race.037",
      "mc001.race.039",
      "mc001.race.040",
      "mc001.race.041",
      "mc001.race.042",
      "mc001.race.043",
      "mc001.race.044",
      "mc001.race.045",
      "mc001.race.046",
      "mc001.race.047"
    },
    tTimerParams = {nStartTime = kGoalTime},
    nAddTime = 10,
    bUseCountdown = false,
    fOnComplete = function()
      MrxVoSequence.Start({
        "Eva-In-Mission-Contract-Mech01-45",
        "Eva-In-Mission-Contract-Mech01-46",
        {
          _CreateDeliverObjective,
          {self}
        }
      })
      self:_PlayerOutside()
    end,
    fOnCancel = function()
      if Object.IsAlive(self.uCar) then
        self:_SetCancelMessage("[MecCon001.Terms.Cancel04]")
        self:Cancel()
      end
    end
  })
  self:SetupPipeTrap("mc001.trap.trigger01", {
    mc001pipetrap11 = "mc001.loc.pipetrap11"
  })
  self:SetupPipeTrap("mc001.trap.trigger02", {
    mc001pipetrap21 = "mc001.loc.pipetrap21"
  })
  self:SetupPipeTrap("mc001.trap.trigger03", {
    mc001pipetrap31 = "mc001.loc.pipetrap31"
  })
  self:SetupBridgeTrap()
  self:SetupProxVo("mc001.race.029", 25, {
    "Fiona-In-Mission-Contract-Mech01-71"
  })
  self:SetupProxVo("mc001.trap.trigger01", 8, {
    "Fiona-In-Mission-Contract-Mech01-69"
  })
  self:SetupProxVo("mc001.trap.trigger02", 6, {
    "Fiona-In-Mission-Contract-Mech01-70"
  })
  self:SetupProxVo("mc001.turnrighwarn", 25, {
    "Fiona-In-Mission-Contract-Mech01-66"
  })
  self:SetupProxVo("mc001.bridgeWarn", 25, {
    "Fiona-In-Mission-Contract-Mech01-67"
  })
  local uGate = Pg.GetGuidByName("vz_mine_gate")
  Object.OpenGate(uGate)
  MrxMusic.PlaySpecialMusic("mu_mission_meccon001_02")
end

function SpawnOpponent(self, sStartName, fHaste)
  local uPos = Pg.GetGuidByName(sStartName)
  local x, y, z = Object.GetPosition(uPos)
  local yaw = Object.GetYaw(uPos)
  fHaste = fHaste or 1
  local uBike = Pg.Spawn("Offroad Motorcycle (AI ONLY)", x, y, z, yaw, false, true)
  table.insert(self.tOpponents, uBike)
  self:_CreateEvent(Event.ObjectHibernation, {uBike, "awake"}, StartOpponent, {
    self,
    uBike,
    fHaste
  })
end

function StartOpponent(self, uBike, fHaste)
  local uDriver = Vehicle.GetDriver(uBike)
  local tPaths = {
    "mc001.pth.bike.001",
    "mc001.pth.bike.002",
    "mc001.pth.bike.003",
    "mc001.pth.bike.004",
    "mc001.pth.bike.005"
  }
  self:OpponentAdvancePath(uDriver, tPaths, fHaste, 1)
end

function OpponentAdvancePath(self, uDriver, tPaths, fHaste, iPathIdx)
  local sPathName = tPaths[iPathIdx]
  if sPathName then
    local uPath = Pg.GetGuidByName(sPathName)
    Junk.DrawPath(uPath)
    self.curAiGoal = Ai.Goal({
      AIGuid = uDriver,
      Goal = "PathMove",
      Target = uPath,
      Haste = fHaste,
      Priority = "HiPri",
      Timeout = 10,
      Callback = OpponentAdvancePath,
      CallbackData = {
        self,
        uDriver,
        tPaths,
        fHaste,
        iPathIdx + 1
      }
    })
  else
    self.bOpponentFinished = true
  end
end

function SetupPipeTrap(self, sTriggerPoint, tPipes)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName(sTriggerPoint),
    "<",
    12,
    false,
    false
  }, TriggerPipeTrap, {self, tPipes})
end

function TriggerPipeTrap(self, tPipes)
  for pipeName, pointName in pairs(tPipes) do
    Object.Kill(Pg.GetGuidByName(pipeName))
  end
  self:_CreateEvent(Event.TimerRelative, {0.1}, SpawnExplosionPipeTrap, {self, tPipes})
end

function SpawnExplosionPipeTrap(self, tPipes)
  for pipeName, pointName in pairs(tPipes) do
    local uPos = Pg.GetGuidByName(pointName)
    local x, y, z = Object.GetPosition(uPos)
    Pg.Spawn("Explosion (Rocket Artillery)", x, y, z, 0, false, false)
    local uPipe = Pg.GetGuidByName(pipeName)
    Net.SendCustomEvent("MecCon001", NETEVENT_SPAWNEXPLOSION, {uPipe})
  end
end

function SetupBridgeTrap(self)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("mc001.bridgeBomb.tigger"),
    "<",
    30,
    false,
    false
  }, TriggerBridgeTrap, {self})
end

function TriggerBridgeTrap(self)
  MrxVoSequence.Start({
    nBaseDelay = 0,
    "Fiona-In-Mission-Contract-Mech01-68"
  })
end

function SetupProxVo(self, sPoint, fRange, tVo)
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName(sPoint),
    "<",
    fRange,
    false,
    false
  }, MrxVoSequence.Start, {tVo})
end

function _CreateDeliverObjective(self)
  self:CreateChild({
    sName = "Park inside the garage",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = self.uCar,
    vDestLoc = "mechanicHQ_loc_delivery",
    vDestRegion = "mechanicHQ.rgn.inside",
    fDist = 3,
    bStop = true,
    bXZOnly = false,
    sDspShortDesc = "[MecCon001.Objectives.park]",
    bDspMsgUpd = false,
    bDisplayHelpText = true,
    nQuota = 1,
    fOnComplete = function()
      self:_ExitGarage()
    end,
    fOnCancel = function()
      if Object.IsAlive(self.uCar) then
        self:Cancel()
      end
    end
  })
end

function _PlayerOutside(self)
  Object.CloseGate(self.garage)
  self._tEvents.eDoorTrigger = Event.Create(Event.Boundary, {
    Player.GetAnyCharacter(),
    self.outRegion,
    "enter",
    false
  }, _PlayerInside, {self})
end

function _PlayerInside(self, uPCharacter)
  Object.OpenGate(self.garage)
  self._tEvents.eDoorTrigger = Event.Create(Event.Boundary, {
    Player.GetAllCharacters(),
    self.outRegion,
    "exit",
    false
  }, _PlayerOutside, {self})
end

function _ExitGarage(self)
  Event.Delete(self._tEvents.eDoorTrigger)
  self._tEvents.eDoorTrigger = Event.Create(Event.Boundary, {
    Player.GetAllCharacters(),
    self.outRegion,
    "exit",
    false
  }, _VehicleDelivered, {self})
end

function _VehicleDelivered(self)
  if Object.InsideBoundary(self.uCar, self.inRegion) then
    Object.CloseGate(self.garage)
    
    local function CompleteMission()
      Event.Delete(self._tEvents.eGateClosed)
      Event.Delete(self._tEvents.eGateStuck)
      Object.Remove(self.uCar)
      self:Complete()
    end
    
    self._tEvents.eGateClosed = Event.Create(Event.ObjectPhysicsEvent, {
      self.garage,
      "gateFullyClosed"
    }, CompleteMission)
    self._tEvents.eGateStuck = Event.Create(Event.ObjectPhysicsEvent, {
      self.garage,
      "gateStuck"
    }, CompleteMission)
  else
    self:_CreateDeliverObjective()
  end
end

function Complete2(self)
  MrxTaskContract.Complete2(self)
  Hud.EventFanfare:Commence({
    sType = "stockpile",
    sText = "[flagpmc][weapon.grapple]"
  })
end

NETEVENT_ENTERVEHICLE = 0
NETEVENT_SPAWNEXPLOSION = 5
NETEVENT_SHOWTUT = 10
NETEVENT_HIDETUT = 11
NETEVENT_CLIENTTUTCOMPLETE = 12

function NetClientEnterVehicle(uVeh)
  local uSecondaryChar = Player.GetSecondaryCharacter()
  local uCurrentVehicleGuid = Vehicle.GetFromRider(uSecondaryChar)
  if Net.IsReadyToTether() and not uCurrentVehicleGuid then
    local res = Vehicle.Enter(uVeh, uSecondaryChar, "d", true, false)
    if type(uVeh) == "userdata" then
    end
  elseif uCurrentVehicleGuid and uVeh ~= uCurrentVehicleGuid then
    Event.Create(Event.ObjectInSeat, {
      uSecondaryChar,
      uCurrentVehicleGuid,
      "a",
      "xo"
    }, NetClientEnterVehicle, {uVeh})
    Vehicle.Exit(uCurrentVehicleGuid, uSecondaryChar)
  else
    Event.Create(Event.TimerRelative, {0.2}, NetClientEnterVehicle, {uVeh})
  end
end

function NetEventCallback(eventId, tArgs)
  if eventId == NETEVENT_ENTERVEHICLE then
    NetClientEnterVehicle(tArgs[1])
  elseif eventId == NETEVENT_SPAWNEXPLOSION then
    local x, y, z = Object.GetPosition(tArgs[1])
    Pg.Spawn("Explosion (Rocket Artillery)", x, y, z, 0, false, false)
  elseif eventId == NETEVENT_SHOWTUT then
    MrxTutorialManager.ShowMessage("[MecCon001.Objectives.buttonTray]")
    geJumpTutorial = Event.Create(Event.Button, {
      Player.GetSecondaryPlayer(),
      "rtrigger",
      "press",
      true
    }, ClientTutorialComplete)
  elseif eventId == NETEVENT_HIDETUT then
    Event.Delete(geJumpTutorial)
    geJumpTutorial = nil
    MrxTutorialManager.HideMessage()
  elseif eventId == NETEVENT_CLIENTTUTCOMPLETE then
    local mission = MrxPlayState.GetCurrentMission()
    mission:TutorialComplete()
  end
end

function ClientTutorialComplete()
  geJumpTutorial = nil
  MrxTutorialManager.HideMessage()
  Net.SendCustomEvent("MecCon001", NETEVENT_CLIENTTUTCOMPLETE, {})
end
