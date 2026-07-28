inherit("MrxTaskJobDestroySet")
import("MrxLayerManager")
_tTargetNearbyVo = {
  {
    vSequence = "Fiona-None-Freeplay-None-02",
    tRange = {
      "[",
      1,
      2,
      "]"
    }
  },
  {
    vSequence = "Fiona-None-Freeplay-None-12",
    tRange = {
      "[",
      3,
      5,
      "]"
    }
  },
  {
    vSequence = "Fiona-None-Freeplay-None-08",
    tRange = {
      "[",
      6,
      8,
      "]"
    }
  }
}
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-In-Mission-Job-All05-06",
    tRange = {
      "[",
      1,
      2,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-07",
    tRange = {
      "[",
      3,
      7,
      "]"
    }
  },
  {
    vSequence = "Fiona-In-Mission-Job-Chi10-10",
    tRange = {8}
  }
}

function LoadAssets(self, tSaveData)
  self:_AddTarget({
    sTarget = "_city_bld_corner16x16d 0x00123386",
    sPristineLayer = "Vz_State_ChiJob005_A_Pristine",
    sDefenseLayer = "Vz_State_ChiJob005_A_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob005_A_Destroyed",
    sStagingLayer = "Vz_State_ChiJob005_A_Staging"
  })
  self:_AddTarget({
    sTarget = "_caracas_bld_theater01 0x000ef1b1",
    sPristineLayer = "Vz_State_ChiJob005_B_Pristine",
    sDefenseLayer = "Vz_State_ChiJob005_B_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob005_B_Destroyed",
    sStagingLayer = "Vz_State_ChiJob005_B_Staging"
  })
  self:_AddTarget({
    sTarget = "_estate_bld_mansion03 0x000b277c",
    sPristineLayer = "Vz_State_ChiJob005_C_Pristine",
    sDefenseLayer = "Vz_State_ChiJob005_C_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob005_C_Destroyed",
    sStagingLayer = "Vz_State_ChiJob005_C_Staging"
  })
  self:_AddTarget({
    sTarget = "_caracas_bld_hospitalvargas 0x000a1b04",
    sPristineLayer = "Vz_State_ChiJob005_D_Pristine",
    sDefenseLayer = "Vz_State_ChiJob005_D_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob005_D_Destroyed",
    sStagingLayer = "Vz_State_ChiJob005_D_Staging"
  })
  self:_AddTarget({
    sTarget = "_caracas_bld_historical04 0x000a1a4a",
    sPristineLayer = "Vz_State_ChiJob005_E_Pristine",
    sDefenseLayer = "Vz_State_ChiJob005_E_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob005_E_Destroyed",
    sStagingLayer = "Vz_State_ChiJob005_E_Staging"
  })
  self:_AddTarget({
    sTarget = "_estate_bld_mansion01 0x000b279c",
    sPristineLayer = "Vz_State_ChiJob005_F_Pristine",
    sDefenseLayer = "Vz_State_ChiJob005_F_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob005_F_Destroyed",
    sStagingLayer = "Vz_State_ChiJob005_F_Staging"
  })
  self:_AddTarget({
    sTarget = "_estate_bld_mansion03 0x000a1d31",
    sPristineLayer = "Vz_State_ChiJob005_G_Pristine",
    sDefenseLayer = "Vz_State_ChiJob005_G_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob005_G_Destroyed",
    sStagingLayer = "Vz_State_ChiJob005_G_Staging"
  })
  self:_AddTarget({
    sTarget = "demo_obj_oilrig 0x0009878a",
    sPristineLayer = "Vz_State_ChiJob009_A_Pristine",
    sDefenseLayer = "Vz_State_ChiJob009_A_Defenses",
    sDestroyedLayer = "Vz_State_ChiJob009_A_Destroyed",
    sStagingLayer = "Vz_State_ChiJob009_A_Staging"
  })
  MrxLayerManager.MarkForAddition("Vz_State_ChiJob009_A_Pristine_tg")
  MrxTaskJobDestroySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobDestroySet.Activated(self)
  self:_SetTargetNearbyVo(_tTargetNearbyVo)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end

function Cleanup(self)
  MrxTaskJobDestroySet.Cleanup(self)
end
