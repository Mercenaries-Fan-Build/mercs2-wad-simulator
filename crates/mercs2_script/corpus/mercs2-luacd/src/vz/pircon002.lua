inherit("MrxTaskContract")
import("MrxSubtitle")
import("MrxFactionManager")

function Activated(self)
  MrxTaskContract.Activated(self)
  nPlayedVO1 = 0
  nPlayedVO2 = 0
  nPlayedVO3 = 0
  nPlayedVO4 = 0
  nGoods = 24
  nGoods1 = 24
  nGoodCopy = 24
  bClientwasIn = false
  nGoods2 = 0
  nCargoValue = 950
  nGoodsDelivered = 0
  nRequired = 5
  tPickups = {
    "JugPickup_1"
  }
  if Player.IsCoopMultiplayer() then
    SetupMPGame(self)
  end
  local nCompletions = self:GetNumCompletions()
  if 2 <= nCompletions then
    nRequired = 16
    nRequiredCash = nRequired * nCargoValue
    tStartTalk = {
      "Fiona-In-Mission-MinorContract-Pir02-23",
      2,
      "Fiona-In-Mission-MinorContract-Pir02-24"
    }
  elseif nCompletions == 1 then
    nRequired = 9
    nRequiredCash = nRequired * nCargoValue
    tStartTalk = {
      "Fiona-In-Mission-MinorContract-Pir02-22",
      2,
      "Fiona-In-Mission-MinorContract-Pir02-25"
    }
  else
    nRequired = 5
    nRequiredCash = nRequired * nCargoValue
    tStartTalk = {
      "Fiona-Banter-MinorContract-Pir03-01",
      0.2,
      {
        mattias = "mattias-Banter-MinorContract-Pir03-02",
        jennifer = "jennifer-Banter-MinorContract-Pir03-03",
        chris = "chris-Banter-MinorContract-Pir03-04"
      },
      "Fiona-Banter-MinorContract-Pir03-05"
    }
  end
  oGotoVehicle = self:CreateChild({
    sName = "PirCon002: GotoVehicle",
    sModuleName = "MrxTaskObjectiveEnterVehicle",
    vTgtInclude = tPickups,
    nQuota = 1,
    vVoSeqOnAdd = tStartTalk,
    sDspShortDesc = "[PirCon002.Objectives.001]",
    tOnComplete = {
      {
        DeliverObjective,
        {self}
      }
    },
    fOnCancel = function()
      self:_SetCancelMessage("[PirCon003.Terms.Cancel03]")
      MrxMusic.StopSpecialMusic("none")
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Oil020-07"
      })
      self:_CreateEvent(Event.TimerRelative, {3}, self.Cancel, {self})
    end
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("JugPickup_1"),
    "<",
    30,
    false,
    false
  }, StartCheckingCargo, {self})
end

function StartCheckingCargo(self)
  self:_CreatePersistentEvent(Event.TimerRelative, {2}, CheckGoodsLost, {self})
end

function DeliverObjective(self)
  MrxMusic.PlaySpecialMusic("mu_mission_pircon002_03")
  local nCompletions = self:GetNumCompletions()
  if 2 <= nCompletions then
    Debug.Printf("+++++++++++++++++++++++++++  Final difficulty!!!!!! ! !1 !")
    sScaleObj = "[PirCon002.Objectives.hard]"
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "EXT (DriverGunner)",
            1
          },
          {
            "Car",
            "Guntruck (OC)(Full)",
            1
          },
          {
            "Heli",
            "Coanda Superiority (Driver)",
            1
          }
        },
        {
          {"Car", 4},
          {"Heli", 1}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "EXT (DriverGunner)",
            1
          },
          {
            "Heli",
            "Coanda Superiority (Driver)",
            1
          }
        },
        {
          {"Car", 5},
          {"Heli", 1}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "EXT (DriverGunner)",
            1
          },
          {
            "Heli",
            "Coanda Superiority (Driver)",
            1
          }
        },
        {
          {"Car", 5},
          {"Heli", 1}
        }
      },
      {
        "Heli",
        {
          {
            "Heli",
            "Coanda Superiority (Driver)",
            1
          }
        },
        {
          {"Heli", 1}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("OC"), -1, tPursuitTable)
  elseif nCompletions == 1 then
    Debug.Printf("+++++++++++++++++++++++++++  This is the Second run 222222222222222222")
    sScaleObj = "[PirCon002.Objectives.med]"
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "EXT (DriverGunner)",
            1
          },
          {
            "Car",
            "Guntruck (OC) (Full)",
            1
          }
        },
        {
          {"Car", 4}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "EXT (DriverGunner)",
            1
          }
        },
        {
          {"Car", 5}
        }
      },
      {
        "Offroad",
        {
          {
            "Car",
            "EXT (DriverGunner)",
            1
          }
        },
        {
          {"Car", 2}
        }
      },
      {
        "Heli",
        {
          {
            "Heli",
            "Coanda Superiority (Driver)",
            1
          }
        },
        {
          {"Heli", 1}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("OC"), -1, tPursuitTable)
  else
    Debug.Printf("+++++++++++++++++++++++++++  This is the first run 000000000000")
    sScaleObj = "[PirCon002.Objectives.easy]"
    local tPursuitTable = {
      {
        "Driving",
        {
          {
            "Car",
            "EXT (DriverGunner)",
            1
          }
        },
        {
          {"Car", 2}
        }
      },
      {
        "Stopped",
        {
          {
            "Car",
            "EXT (DriverGunner)",
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
            "EXT (DriverGunner)",
            1
          }
        },
        {
          {"Car", 1}
        }
      }
    }
    MrxFactionManager.SetCustomPursuit(Pg.GetGuidByName("OC"), -1, tPursuitTable)
  end
  sHText = "[PirCon003.Objectives.Req]" .. MrxUtil.FormatMoney(nRequiredCash) .. " [PirCon003.Objectives.ReqB]"
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 2,
    sText = sHText
  })
  DisplayGoodsLost(self)
  self:CreateChild({
    sName = "PirCon002: DeliverProps",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = tPickups,
    nQuota = 1,
    vDestLoc = "loc_PirCon002deliver",
    fDist = 15,
    bStop = true,
    bXZOnly = false,
    sDspShortDesc = sScaleObj,
    fOnComplete = function()
      if bClientwasIn then
        ActivateDelivered(self)
      else
        CountDelivered(self)
      end
    end,
    fOnCancel = function()
      self:_SetCancelMessage("[PirCon003.Terms.Cancel03]")
      MrxMusic.StopSpecialMusic("none")
      MrxVoSequence.Start({
        "Fiona-In-Mission-Contract-Oil020-07"
      })
      self:_CreateEvent(Event.TimerRelative, {4}, self.Cancel, {self})
    end
  })
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Pir02-21",
    "Fiona-In-Mission-MinorContract-Pir03-01"
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("JugPickup_1"),
    Pg.GetGuidByName("loc_PirCon002deliver"),
    "<",
    250,
    false,
    false
  }, MrxFactionManager.ClearPursuitLock, {})
  self:_CreateEvent(Event.ObjectProximity, {
    Pg.GetGuidByName("JugPickup_1"),
    Pg.GetGuidByName("loc_PirCon002deliver"),
    "<",
    120,
    false,
    false
  }, MrxMusic.StopSpecialMusic, {"none"})
end

function CheckGoodsLost(self)
  local nGoodCopy = nGoods
  local uMainTruck = Pg.GetGuidByName("JugPickup_1")
  if uMainTruck then
    if Object.IsAlive(uMainTruck) then
      if not MrxUtil.TestDistanceToAllPlayers(uMainTruck, 45, false, true) then
        self:_CreateEvent(Event.TimerRelative, {1}, function(self)
          if not MrxUtil.TestDistanceToAllPlayers(uMainTruck, 45, false, true) then
            local x, y, z = Object.GetHardpointPosition(uMainTruck, "HP_Truckbed")
            if x and y and z then
              local tGoods = Pg.GetObjectsInArea(x, y, z, 1, "RumJug")
              nGoods1 = table.getn(tGoods)
            end
          end
        end, {self})
      end
    else
      nGoods1 = 0
    end
  end
  if bClientwasIn then
    local uBonusTruck = Pg.GetGuidByName("JugPickup_2")
    if uBonusTruck then
      if Object.IsAlive(uBonusTruck) then
        if not MrxUtil.TestDistanceToAllPlayers(uBonusTruck, 45, false, true) then
          self:_CreateEvent(Event.TimerRelative, {1}, function(self)
            if not MrxUtil.TestDistanceToAllPlayers(uMainTruck, 45, false, true) then
              local x, y, z = Object.GetHardpointPosition(uBonusTruck, "HP_Truckbed")
              if x and y and z then
                local tGoods2 = Pg.GetObjectsInArea(x, y, z, 1, "RumJug")
                nGoods2 = table.getn(tGoods2)
              end
            end
          end, {self})
        end
      else
        nGoods2 = 0
      end
    end
  end
  nGoods = nGoods1 + nGoods2
  if nGoodCopy > nGoods then
    DisplayGoodsLost(self)
  end
end

function _PlayRandomVO(self, tVOs)
  local i = math.randi(table.getn(tVOs))
  self:_PlayVo(0, tVOs[i])
end

function DisplayGoodsLost(self)
  nCash = nGoods * nCargoValue
  sHudText = "[0x85070644]: " .. MrxUtil.FormatMoney(nCash)
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 1,
    sText = sHudText
  })
  if nGoods < nRequired then
    if nPlayedVO4 == 0 then
      self:_SetCancelMessage("[PirCon002.Terms.Cancel01]")
      MrxMusic.StopSpecialMusic("none")
      local tVo = {
        "Fiona-In-Mission-MinorContract-Pir02-28",
        "Fiona-In-Mission-MinorContract-Pir02-29",
        "Fiona-In-Mission-MinorContract-Pir02-30"
      }
      local sSelectedVo = MrxUtil.GetRandomTableElement(tVo)
      local tSequence = {
        sSelectedVo,
        {
          self.Cancel,
          {self}
        }
      }
      MrxVoSequence.Start(tSequence)
      nPlayedVO4 = 1
    end
  elseif nGoods == 18 then
    if nPlayedVO1 == 0 then
      MrxVoSequence.Start({
        "Fiona-In-Mission-MinorContract-Pir02-10"
      })
      nPlayedVO1 = 1
    end
  elseif nGoods == 11 then
    if nPlayedVO2 == 0 then
      MrxVoSequence.Start({
        "Fiona-In-Mission-MinorContract-Pir02-11"
      })
      nPlayedVO2 = 1
    end
  elseif nGoods == 6 and nPlayedVO3 == 0 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pir02-02"
    })
    nPlayedVO3 = 1
  end
end

function ActivateDelivered(self)
  self:CreateChild({
    sName = "PirCon002 MP End",
    sModuleName = "MrxTaskObjectiveAction",
    vTgtInclude = "PirCon002_endTalk",
    sDspShortDesc = "[PirCon003.Objectives.end]",
    tOnComplete = {
      {
        CountDelivered,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function CountDelivered(self)
  MrxMusic.StopSpecialMusic("none")
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("loc_PirCon002deliver"))
  local tGoodsDelivered = {}
  local tGoodsDelivered = Pg.GetObjectsInArea(x, y, z, 25, "RumJug")
  local nGoodsDelivered = table.getn(tGoodsDelivered)
  local iCashReward = nGoodsDelivered * nCargoValue
  if nGoodsDelivered == 24 and not bClientwasIn then
    self:_SetPlayer1Bonus(2000000 + iCashReward)
  elseif nGoodsDelivered == 48 and bClientwasIn then
    local MPBonus = 2000000 + iCashReward / 2
    self:_SetPlayer1Bonus(MPBonus)
    self:_SetPlayer2Bonus(MPBonus)
  elseif Net.IsActive() then
    self:_SetPlayer1Bonus(iCashReward / 2)
    self:_SetPlayer2Bonus(iCashReward / 2)
  else
    self:_SetPlayer1Bonus(iCashReward)
  end
  if 1 <= nGoodsDelivered then
    MrxVoSequence.Start({
      "PirThug(female)-In-Mission-MinorContract-Pir02-19",
      "Fiona-In-Mission-MinorContract-Pir03-04",
      "PirThug(female)-In-Mission-MinorContract-Pir02-18",
      {
        self.Complete,
        {self}
      }
    })
  else
    self:_SetCancelMessage("[PirCon002.Terms.Cancel01]")
    MrxVoSequence.Start({
      "PirThug-In-Mission-MinorContract-Pir02-07",
      {
        self.Cancel,
        {self}
      }
    })
  end
end

function SetupMPGame(self)
  tLayersToAdd = {
    "vz_State_PirCon002_MP"
  }
  MrxLayerManager.Add(tLayersToAdd)
  tPickups = {
    "JugPickup_1",
    "JugPickup_2"
  }
  bClientwasIn = true
  nGoods2 = 24
  nGoodCopy = 48
  self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("PirCon002_endTalk")
  }, function(self)
    self:_SetCancelMessage("[PirCon003.Terms.Cancel04]")
    self:Cancel()
  end, {self})
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("PirCon002_endTalk"),
    "awake"
  }, function(self)
    Ai.Goal({
      AIGuid = Pg.GetGuidByName("PirCon002_endTalk"),
      Goal = "Idle",
      Priority = "hiPri"
    })
  end, {self})
end

function Cleanup(self)
  local nCompletions = self:GetNumCompletions()
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  MrxLayerManager.Remove("vz_State_PirCon002_Deliverables")
  if bClientwasIn then
    MrxLayerManager.Remove("vz_state_PirCon002_MP")
  end
  MrxFactionManager.ClearCustomPursuit()
  MrxMusic.StopSpecialMusic("none")
  MrxTaskContract.Cleanup(self)
end
