inherit("MrxTaskContract")
import("MrxFactionManager")
import("MrxVoSequence")
import("MrxTaskRace")
import("MrxTutorialManager")

function LoadAssets(self, tSaveData)
  tLayersToRemove = {
    "vz_state_car_city_act1",
    "vz_state_mar_city_act1",
    "vz_state_staging_pirhq"
  }
  MrxLayerManager.Remove(tLayersToRemove, function()
    local tLayersToAdd = {
      "VZ_State_GurCon003"
    }
    MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  end)
end

function Activated(self)
  MrxTaskContract.Activated(self)
  tInitialVOTable = {}
  nPurLevel = 1
  nTimerLevel = 180
  bClientwasIn = false
  bMusicStarted = false
  nTutePlayed = 0
  local spawnBoat = Pg.GetGuidByName("GurCon003_deliv")
  local uYaw = Object.GetYaw(spawnBoat)
  local x, y, z = Object.GetPosition(spawnBoat)
  uPiranha = Pg.Spawn("Piranha", x, y, z, uYaw)
  tPiranha = {uPiranha}
  if Player.IsCoopMultiplayer() then
    SetupMPGame(self)
  end
  local nCompletions = self:GetNumCompletions()
  if 2 <= nCompletions then
    tInitialVOTable = {
      "Fiona-In-Mission-MinorContract-Gur03-20"
    }
    Debug.Printf("+++++++++++++++++++++++++++  Final difficulty!!!!!! ! !1 !")
    nPurLevel = 3
    nTimerLevel = 120
    local tLayersToAdd = {
      "VZ_State_GurCon003_Med"
    }
    MrxLayerManager.Add(tLayersToAdd)
  elseif nCompletions == 1 then
    tInitialVOTable = {
      "Fiona-In-Mission-MinorContract-Gur03-21"
    }
    local tLayersToAdd = {
      "VZ_State_GurCon003_Med"
    }
    MrxLayerManager.Add(tLayersToAdd)
    nPurLevel = 2
    nTimerLevel = 150
    Debug.Printf("+++++++++++++++++++++++++++ This is the Second run 222222222222222222")
  else
    Debug.Printf("+++++++++++++++++++++++++++  This is the first run 000000000000")
    tInitialVOTable = {
      "Fiona-Banter-MinorContract-Gur03-01",
      0.5,
      {
        mattias = "mattias-Banter-MinorContract-Gur03-02",
        jennifer = "jennifer-Banter-MinorContract-Gur03-03",
        chris = "chris-Banter-MinorContract-Gur03-04"
      },
      0.5,
      "Fiona-Banter-MinorContract-Gur03-05"
    }
    nPurLevel = 1
    nTimerLevel = 300
    CreateTutorialTrigger(self)
  end
  oRaceObj = self:CreateChild({
    sRaceMission = "GurCon003",
    sName = "DeliverBoat",
    sModuleName = "MrxTaskRace",
    sDspShortDesc = "[GurCon003.Objectives.002]",
    vTgtInclude = tPiranha,
    vVoSeqOnAdd = tInitialVOTable,
    fWidth = 30,
    tTimerParams = {
      nStartTime = 50 - nPurLevel * 5
    },
    nAddTime = 17 - nPurLevel * 2,
    tCourseLocs = {
      "GurCon3_gate_02",
      "GurCon3_gate_05",
      "GurCon3_gate_10",
      "GurCon3_gate_12",
      "GurCon3_gate_15",
      "GurCon3_gate_20",
      "GurCon3_gate_28",
      "GurCon3_gate_30",
      "GurCon3_gate_35",
      "GurCon3_gate_37",
      "GurCon3_gate_40",
      "GurCon3_gate_49",
      "GurCon3_gate_51",
      "GurCon3_gate_54",
      "GurCon3_gate_55",
      "GurCon3_gate_56",
      "GurCon3_gate_57",
      "GurCon3_gate_60",
      "GurCon3_gate_63",
      "GurCon3_gate_65",
      "GurCon3_gate_70"
    },
    fOnComplete = function()
      if Object.GetHealth(uPiranha) >= 50 and not MrxUtil.TestDistanceToAllPlayers(uPiranha, 60, false, true) then
        MrxVoSequence.Start({
          "ChinaSoldier-In-Mission-MinorContract-Gur03-18"
        })
        if Net.IsActive() then
          self:_SetPlayer1Bonus(500000)
          self:_SetPlayer2Bonus(500000)
        else
          self:_SetPlayer1Bonus(500000)
        end
      end
      if bClientwasIn and 50 <= Object.GetHealth(uPiranhaB) and not MrxUtil.TestDistanceToAllPlayers(uPiranhaB, 60, false, true) then
        MrxVoSequence.Start({
          "ChinaSoldier-In-Mission-MinorContract-Gur03-18"
        })
        if Net.IsActive() then
          self:_SetPlayer1Bonus(500000)
          self:_SetPlayer2Bonus(500000)
        else
          self:_SetPlayer1Bonus(500000)
        end
      end
      self:_CreateEvent(Event.TimerRelative, {4}, self.Complete, {self})
    end,
    fOnCancel = function()
      self:_SetCancelMessage("[GurCon003.Terms.Cancel02]")
      self:_CreateEvent(Event.TimerRelative, {4}, self.Cancel, {self})
    end
  })
  _SetupBonusObjective(self)
  for i = 1, 5 do
    uBoatBlock = Pg.GetGuidByName("BoatBlock_" .. i)
    if uBoatBlock then
      self:_CreateEvent(Event.ObjectHibernation, {uBoatBlock, "awake"}, RiverMovers, {self, i})
    end
  end
  for Boat = 1, 4 do
    self:_CreateEvent(Event.ObjectHibernation, {
      Pg.GetGuidByName("Finder_" .. Boat),
      "awake"
    }, FinderMovers, {self, Boat})
  end
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("1stmine"),
    "<",
    50,
    false,
    false
  }, MineTalk, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("GurCon3_gate_40"),
    "<",
    15,
    false,
    false
  }, SetTime, {self})
  if nPurLevel == 1 then
    self:_CreateEvent(Event.ObjectProximity, {
      uPiranha,
      Pg.GetGuidByName("GurCon3_gate_20"),
      "<",
      40,
      false,
      false
    }, SecondTute, {self})
  end
  self:_CreateEvent(Event.ObjectProximity, {
    uPiranha,
    Pg.GetGuidByName("GurCon003_dest"),
    "<",
    400,
    false,
    false
  }, EndPursuit, {self})
  uBoatDeath = self:_CreateEvent(Event.ObjectDeath, {uPiranha}, BoatDestroyed, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("ChinaContact")
  }, ContactKilled, {self})
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("ChinaDriver")
  }, ContactKilled, {self})
  for i, uPiran in ipairs(tPiranha) do
    self:_CreateEvent(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      uPiran,
      "A",
      "E"
    }, PlayMusic, {self})
  end
  self:_CreateEvent(Event.ObjectProximity, {
    uPiranha,
    Pg.GetGuidByName("loc_StartOCBoats"),
    "<",
    140,
    false,
    false
  }, StartPursuit, {self, nPurLevel})
end

function SecondTute(self)
  if Object.IsPlayerControlled(uPiranha) then
    MrxTutorialManager.BeginCustomTutorial("GurCon003")
    MrxTutorialManager.ShowMessage("[GurCon003.Terms.buttontray]", false, "GurCon003")
    self:_CreateEvent(Event.TimerRelative, {9}, TutorialCancel, {self})
  end
end

function _SetupBonusObjective(self)
  if bClientwasIn then
    nBonusMP = 2
  else
    nBonusMP = 1
  end
  Debug.Printf("inside _SetupBonusObjective")
  oBoatHealthBonus = self:CreateChild({
    sName = "Piranha bonus",
    sModuleName = "MrxTaskObjective",
    bOptional = true,
    bDspMsg = false,
    bDspDescPda = true,
    bDspBlp = false,
    sDspShortDesc = "[GurCon003.Objectives.003]"
  })
  for i, uBoatPira in ipairs(tPiranha) do
    uBonusEvent = self:_CreateEvent(Event.ObjectHealthLessThan, {uBoatPira, 49}, CheckBoats, {self})
  end
end

function CheckBoats(self)
  nBonusMP = nBonusMP - 1
  if nBonusMP == 0 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Gur03-19"
    })
    oBoatHealthBonus:Cancel()
  end
end

function SetTime(self)
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 0 then
    oRaceObj.nAddTime = 12
    oRaceObj.fWidth = 18
  elseif nCompletions == 1 then
    oRaceObj.nAddTime = 9
    oRaceObj.fWidth = 14
  elseif 2 <= nCompletions then
    oRaceObj.nAddTime = 8
    oRaceObj.fWidth = 10
  end
end

function PlayMusic(self)
  if bMusicStarted == false then
    MrxMusic.PlaySpecialMusic("mu_fac_gr_kickass_01")
    bMusicStarted = true
  end
end

function SetupMPGame(self)
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("GurCon003_deliv_2"))
  local nFace = Object.GetYaw(Pg.GetGuidByName("GurCon003_deliv_2"))
  uPiranhaB = Pg.Spawn("Piranha", x, y, z, nFace, true, true)
  table.insert(tPiranha, uPiranhaB)
  bClientwasIn = true
end

function CreateTutorialTrigger(self)
  if eSteppedOut then
    Event.Delete(eSteppedOut)
  end
  if eTutorialExit then
    Event.Delete(eTutorialExit)
  end
  MrxTutorialManager.EndCustomTutorial("GurCon003")
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    uPiranha,
    "d",
    "ei"
  }, StartTutorial, {self})
end

function StartTutorial(self, uChar, uVehicle)
  SetupJumpTutorial(self, uChar, uVehicle)
  self:_CreateEvent(Event.TimerRelative, {0.5}, SetupTutorialTray, {
    self,
    uChar,
    uVehicle
  })
  if nTutePlayed == 0 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Gur03-02"
    })
    nTutePlayed = 1
  end
end

function SetupJumpTutorial(self, uChar, uVehicle)
  local uDriver = Vehicle.GetDriver(uVehicle)
  if uDriver then
    local uPlayer = Object.IsPlayerControlled(uDriver)
    eTutorialEnd = self:_CreateEvent(Event.Button, {
      uPlayer,
      "lbutton",
      "press",
      true
    }, TutorialCancel, {self})
    eTutorialExit = self:_CreateEvent(Event.ObjectInSeat, {
      uDriver,
      uVehicle,
      "d",
      "xo"
    }, CreateTutorialTrigger, {self})
  else
    Event.Delete(eTutorialEnd)
    eSteppedOut = self:_CreateEvent(Event.ObjectInSeat, {
      Player.GetAnyCharacter(),
      uVehicle,
      "d",
      "ei"
    }, SetupJumpTutorial, {self, true})
  end
end

function SetupTutorialTray(self, uChar, uVehicle)
  local uDriver = Vehicle.GetDriver(uVehicle)
  if uDriver then
    local uPlayer = Object.IsPlayerControlled(uDriver)
    MrxTutorialManager.BeginCustomTutorial("GurCon003")
    MrxTutorialManager.ShowMessage("[GurCon003.Terms.buttontray]", false, "GurCon003")
  end
end

function TutorialCancel(self)
  MrxTutorialManager.EndCustomTutorial("GurCon003")
  if eSteppedOut then
    Event.Delete(eSteppedOut)
  end
  if eTutorialExit then
    Event.Delete(eTutorialExit)
  end
end

function BoatDestroyed(self)
  self:_SetCancelMessage("[GurCon003.Terms.Cancel01]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Gur03-12"
  })
  self:_CreateEvent(Event.TimerRelative, {3}, self.Cancel, {self})
end

function ContactKilled(self)
  self:_SetCancelMessage("[GurCon003.Terms.Cancel03]")
  self:Cancel()
end

function StartPursuit(self, nPurLevel)
  if nPurLevel == 1 then
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 3}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 3}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 2}
        }
      },
      {
        "Boat",
        {
          {
            "Boat",
            "Omen (OC) (DriverGunner)",
            1
          },
          {
            "Boat",
            "Turbosquid (OC) (Full)",
            2
          }
        },
        {
          {"Boat", 5}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("OC"), -1, tPursuitTable)
  elseif nPurLevel == 2 then
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 3}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 3}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 2}
        }
      },
      {
        "Boat",
        {
          {
            "Boat",
            "Omen (OC) (DriverGunner)",
            1
          },
          {
            "Boat",
            "Turbosquid (OC) (Full)",
            1
          }
        },
        {
          {"Boat", 6}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("OC"), -1, tPursuitTable)
  elseif nPurLevel == 3 then
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 1}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 1}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "EXT (GL) (DriverGunner)",
            1
          }
        },
        {
          {"Car", 1}
        }
      },
      {
        "Boat",
        {
          {
            "Heli",
            "Coanda Gunship (Driver)",
            1
          }
        },
        {
          {"Heli", 2}
        }
      },
      {
        "Boat",
        {
          {
            "Boat",
            "Omen (OC) (DriverGunner)",
            1
          },
          {
            "Boat",
            "Turbosquid (OC) (Full)",
            1
          }
        },
        {
          {"Boat", 6}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("OC"), -1, tPursuitTable)
  end
end

function EndPursuit(self)
  MrxFactionManager.ClearCustomPursuit()
end

function MineTalk(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Gur03-17",
    "Fiona-In-Mission-MinorContract-Gur03-16"
  })
end

function RiverMovers(self, nBoat)
  local uRiverBoat = Pg.GetGuidByName("BoatBlock_" .. nBoat)
  if uRiverBoat then
    local uBoatCapn = Vehicle.GetDriver(uRiverBoat)
    if Object.IsAlive(uBoatCapn) then
      Debug.Printf("Starting the path again")
      Ai.Goal({
        AIGuid = uBoatCapn,
        Goal = "PathMove",
        Target = Pg.GetGuidByName("Pa_BoatBlock_3"),
        Start = "Nearest",
        Priority = "hiPri",
        Mode = "Bounce",
        Callback = RiverMovers,
        CallbackData = {self, nBoat}
      })
    end
  end
end

function FinderMovers(self, nFinder)
  local uBoatCapn = Vehicle.GetDriver(Pg.GetGuidByName("Finder_" .. nFinder))
  if Object.IsAlive(uBoatCapn) then
    Debug.Printf("Starting the path again")
    if nFinder == 4 then
      nFinder = 1
    end
    Ai.Goal({
      AIGuid = uBoatCapn,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Pa_Finder_" .. nFinder),
      Start = "Nearest",
      Priority = "hiPri",
      Callback = FinderMovers,
      CallbackData = {self, nFinder}
    })
    Ai.SetHaste(uBoatCapn, 1)
  end
end

function Cleanup(self)
  Object.Remove(uPiranha)
  if bClientwasIn then
    Object.Remove(uPiranhaB)
  end
  MrxMusic.StopSpecialMusic("none")
  tLayersForAddition = {
    "vz_state_car_city_act1",
    "vz_state_mar_city_act1",
    "vz_state_staging_pirhq"
  }
  MrxLayerManager.Add(tLayersForAddition)
  MrxFactionManager.ClearCustomPursuit()
  MrxTaskContract.Cleanup(self)
end
