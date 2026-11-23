local L0_1, L1_1, L2_1
import("MrxSound", false)
import("MrxMusic", false)
import("MrxSoundCategories", false)
import("MrxSoundBanks", false)

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  Sound.DefineReverbPreset(1, "CITY_KG_LIGHT_REFLECTIONS", -1000, -1000, 0, 0.09, 0.23, -602, 0.02, -698, 0.03, 100, 100, 5000, 0, 5000, -5000)
  L0_2 = Sound._GetLibVersion()
  if 10 <= L0_2 then
    Sound.SetReverbPreset("CITY_KG_LIGHT_REFLECTIONS")
  else
    Sound.SetReverbPreset(1)
  end
  Sound.SetReverb(1)
  MrxSoundCategories.SetPitchCategory("survivalmode", "non_ui", 0.5, 0.5, 0.5)
  MrxSoundCategories.SetPitchCategory("survivalmode", "chatter", 0.75, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("vosequence", "non_ui", 0.3, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("vosequence", "chatter", 0.3, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("vosequence", "music", 0.15, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("actionhijack", "Non_Action_Hijack", 0.4, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("actionhijack", "chatter", 0.3, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("survivalmode", "non_ui", 0.4, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("survivalmode", "chatter", 0.3, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("survivalmode", "music", 0.5, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("fanfare", "non_ui", 0.1, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("fanfare", "vo", 0.1, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("satelliteview", "non_ui", 0.1, 0.5, 0.5)
  MrxSoundCategories.SetFadeCategory("satelliteview", "chatter", 0.1, 0.5, 0.5)
  MrxMusic.BindMusicCue("an", "explore", 1, "mu_fac_an_explore_01")
  MrxMusic.BindMusicCue("an", "action", 1, "mu_fac_an_threat_01")
  MrxMusic.BindMusicCue("an", "mission_success", 1, "mu_fac_an_win_01")
  MrxMusic.BindMusicCue("an", "mission_failure", 1, "mu_fac_an_fail_01")
  MrxMusic.BindMusicCue("an", "hijack", 1, "mu_fac_an_hijack_01")
  MrxMusic.BindMusicCue("an", "hijack", 2, "mu_fac_an_hijack_02")
  MrxMusic.BindMusicCue("an", "hijack", 3, "mu_fac_an_hijack_03")
  MrxMusic.BindMusicCue("an", "hijack_success", 1, "mu_fac_an_kickass_01")
  MrxMusic.BindMusicCue("an", "shell", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("an", "pause", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("oc", "explore", 1, "mu_fac_oc_explore_01")
  MrxMusic.BindMusicCue("oc", "action", 1, "mu_fac_oc_threat_01")
  MrxMusic.BindMusicCue("oc", "mission_success", 1, "mu_fac_oc_win_01")
  MrxMusic.BindMusicCue("oc", "mission_failure", 1, "mu_fac_oc_fail_01")
  MrxMusic.BindMusicCue("oc", "hijack", 1, "mu_fac_oc_hijack_01")
  MrxMusic.BindMusicCue("oc", "hijack", 2, "mu_fac_oc_hijack_02")
  MrxMusic.BindMusicCue("oc", "hijack_success", 1, "mu_fac_oc_kickass_01")
  MrxMusic.BindMusicCue("oc", "shell", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("oc", "pause", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("gr", "explore", 1, "mu_fac_gr_explore_01")
  MrxMusic.BindMusicCue("gr", "action", 1, "mu_fac_gr_threat_01")
  MrxMusic.BindMusicCue("gr", "mission_success", 1, "mu_fac_gr_win_01")
  MrxMusic.BindMusicCue("gr", "mission_failure", 1, "mu_fac_gr_fail_01")
  MrxMusic.BindMusicCue("gr", "hijack", 1, "mu_fac_gr_hijack_01")
  MrxMusic.BindMusicCue("gr", "hijack", 2, "mu_fac_gr_hijack_02")
  MrxMusic.BindMusicCue("gr", "hijack", 3, "mu_fac_gr_hijack_03")
  MrxMusic.BindMusicCue("gr", "hijack_success", 1, "mu_fac_gr_kickass_01")
  MrxMusic.BindMusicCue("gr", "shell", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("gr", "pause", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("ch", "explore", 1, "mu_fac_ch_explore_01")
  MrxMusic.BindMusicCue("ch", "action", 1, "mu_fac_ch_threat_01")
  MrxMusic.BindMusicCue("ch", "mission_success", 1, "mu_fac_ch_win_01")
  MrxMusic.BindMusicCue("ch", "mission_failure", 1, "mu_fac_ch_fail_01")
  MrxMusic.BindMusicCue("ch", "hijack", 1, "mu_fac_ch_hijack_01")
  MrxMusic.BindMusicCue("ch", "hijack", 2, "mu_fac_ch_hijack_02")
  MrxMusic.BindMusicCue("ch", "hijack", 3, "mu_fac_ch_hijack_03")
  MrxMusic.BindMusicCue("ch", "hijack_success", 1, "mu_fac_ch_kickass_01")
  MrxMusic.BindMusicCue("ch", "shell", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("ch", "pause", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("pmc", "explore", 1, "mu_fac_pmc_explore_01")
  MrxMusic.BindMusicCue("pmc", "action", 1, "mu_fac_pmc_threat_01")
  MrxMusic.BindMusicCue("pmc", "mission_success", 1, "mu_fac_pmc_win_01")
  MrxMusic.BindMusicCue("pmc", "mission_failure", 1, "mu_fac_pmc_fail_01")
  MrxMusic.BindMusicCue("pmc", "hijack", 1, "mu_fac_oc_hijack_01")
  MrxMusic.BindMusicCue("pmc", "hijack", 2, "mu_fac_oc_hijack_02")
  MrxMusic.BindMusicCue("pmc", "hijack_success", 1, "mu_fac_pmc_kickass_01")
  MrxMusic.BindMusicCue("pmc", "shell", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("pmc", "pause", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("freeplay_city", "explore", 1, "mu_nomission_city_explore_01")
  MrxMusic.BindMusicCue("freeplay_city", "action", 1, "mu_nomission_city_threat_01")
  MrxMusic.BindMusicCue("freeplay_city", "high_action", 1, "mu_nomission_city_threat_02")
  MrxMusic.BindMusicCue("freeplay_city", "mission_failure", 1, "mu_nomission_city_fail_01")
  MrxMusic.BindMusicCue("freeplay_city", "mission_success", 1, "mu_fac_pmc_win_01")
  MrxMusic.BindMusicCue("freeplay_city", "hijack", 1, "mu_fac_oc_hijack_01")
  MrxMusic.BindMusicCue("freeplay_city", "hijack", 2, "mu_fac_oc_hijack_02")
  MrxMusic.BindMusicCue("freeplay_city", "hijack_success", 1, "mu_fac_pmc_kickass_01")
  MrxMusic.BindMusicCue("freeplay_city", "shell", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("freeplay_city", "pause", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("freeplay_jungle", "explore", 1, "mu_nomission_jungle_explore_01")
  MrxMusic.BindMusicCue("freeplay_jungle", "action", 1, "mu_nomission_jungle_threat_01")
  MrxMusic.BindMusicCue("freeplay_jungle", "high_action", 1, "mu_nomission_jungle_threat_02")
  MrxMusic.BindMusicCue("freeplay_jungle", "mission_failure", 1, "mu_nomission_jungle_fail_01")
  MrxMusic.BindMusicCue("freeplay_jungle", "mission_success", 1, "mu_fac_pmc_win_01")
  MrxMusic.BindMusicCue("freeplay_jungle", "hijack", 1, "mu_fac_oc_hijack_01")
  MrxMusic.BindMusicCue("freeplay_jungle", "hijack", 2, "mu_fac_oc_hijack_02")
  MrxMusic.BindMusicCue("freeplay_jungle", "hijack_success", 1, "mu_fac_pmc_kickass_01")
  MrxMusic.BindMusicCue("freeplay_jungle", "shell", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("freeplay_jungle", "pause", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("freeplay_water", "explore", 1, "mu_nomission_water_explore_01")
  MrxMusic.BindMusicCue("freeplay_water", "action", 1, "mu_nomission_water_threat_01")
  MrxMusic.BindMusicCue("freeplay_water", "high_action", 1, "mu_nomission_water_threat_02")
  MrxMusic.BindMusicCue("freeplay_water", "mission_failure", 1, "mu_nomission_water_fail_01")
  MrxMusic.BindMusicCue("freeplay_water", "mission_success", 1, "mu_fac_pmc_win_01")
  MrxMusic.BindMusicCue("freeplay_water", "hijack", 1, "mu_fac_oc_hijack_01")
  MrxMusic.BindMusicCue("freeplay_water", "hijack", 2, "mu_fac_oc_hijack_02")
  MrxMusic.BindMusicCue("freeplay_water", "hijack_success", 1, "mu_fac_pmc_kickass_01")
  MrxMusic.BindMusicCue("freeplay_water", "shell", 1, "mu_shell_01")
  MrxMusic.BindMusicCue("freeplay_water", "pause", 1, "mu_shell_01")
  MrxSoundCategories.SetDuckOnGlobalTableLoad(true)
  LoadBanks()
  MrxSound.Initialize()
end

Init = L0_1

function L0_1()
  local L0_2, L1_2
  UnloadBanks()
end

ExitGame = L0_1

function L0_1()
  local L0_2, L1_2
  LoadRequiredAssetsDLC()
  LoadResidentAssetsVZ()
  MrxSoundBanks.LoadSoundBank("music")
  MrxSoundBanks.LoadWaveBank("music")
  MrxSoundBanks.LoadWaveBank("ui_hud")
  MrxSoundBanks.LoadSoundBank("ui_hud")
  MrxSoundBanks.LoadWaveBank("DLCTest_streaming")
  MrxSoundBanks.LoadSoundBank("DLCTest_streaming")
  MrxSoundBanks.LoadWaveBank("DLCTest_resident")
  MrxSoundBanks.LoadSoundBank("DLCTest_resident")
  MrxSoundBanks.LoadWaveBank("vo_stream_DLCTest")
  MrxSoundBanks.LoadSoundBank("vo_resident_DLCTest")
end

LoadBanks = L0_1

function L0_1()
  local L0_2, L1_2
  MrxSoundBanks.UnloadSoundBank("music")
  MrxSoundBanks.UnloadWaveBank("music")
  MrxSoundBanks.UnloadWaveBank("ui_hud")
  MrxSoundBanks.UnloadSoundBank("ui_hud")
  MrxSoundBanks.UnloadWaveBank("DLCTest_streaming")
  MrxSoundBanks.UnloadSoundBank("DLCTest_streaming")
  MrxSoundBanks.UnloadWaveBank("DLCTest_resident")
  MrxSoundBanks.UnloadSoundBank("DLCTest_resident")
  MrxSoundBanks.UnloadWaveBank("vo_stream_DLCTest")
  MrxSoundBanks.UnloadSoundBank("vo_resident_DLCTest")
  UnloadResidentAssetsVZ()
  UnloadRequiredAssetsDLC()
end

UnloadBanks = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L1_2 = string.sub(A0_2, 1, 3)
  if L1_2 == "vo_" then
    L3_2 = A0_2 .. "." .. Gui.GetLanguageName()
    return L3_2
  end
  return A0_2
end

_GetLocalizedName = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L1_2 = string.find(A0_2, ".pws")
  L2_2 = string.sub
  L3_2 = A0_2
  L4_2 = 1
  L5_2 = L1_2 - 1
  return L2_2(L3_2, L4_2, L5_2)
end

_StripPWSExtension = L0_1

function L0_1()
  local L0_2, L1_2
  
  function L0_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3
    Sound.OpenStreamFile(((Net.DLCGetMountPoint() .. "\\" .. string.upper(Sound.GetAudioDir())) .. "\\" .. string.upper(_GetLocalizedName(_StripPWSExtension(A0_3))) .. ".PWS"), A0_3)
  end
  
  _OpenFile = L0_2
  _OpenFile("DLCTest_streaming.pws")
  _OpenFile("vo_stream_DLCTest.pws")
end

OpenStreamsDLC = L0_1

function L0_1()
  local L0_2, L1_2
  
  function L0_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3
    Sound.CloseStreamFile(((Net.DLCGetMountPoint() .. "\\" .. string.upper(Sound.GetAudioDir())) .. "\\" .. string.upper(_GetLocalizedName(_StripPWSExtension(A0_3))) .. ".PWS"), A0_3)
  end
  
  _CloseFile = L0_2
  _CloseFile("DLCTest_streaming.pws")
  _CloseFile("vo_stream_DLCTest.pws")
end

CloseStreamsDLC = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  OpenStreamsDLC()
  Pg.LoadAsset("Mercs2Globals", "sounddb", MrxSoundCategories._DuckGlobalTable)
  Pg.LoadAsset("MusicMarkers", "musicmarkers")
  Pg.LoadAsset("MusicTransitions", "musictransitions")
  Pg.LoadAsset("VehicleEngines", "animationtable")
  Pg.LoadAsset("Sounds", "animationtable")
  Pg.LoadAsset("SoundsAppendix", "animationtable")
  Pg.LoadAsset("SoundMatch", "animationtable")
  Pg.LoadAsset("SoundKey", "materialkeytable")
end

LoadRequiredAssetsDLC = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  Pg.UnloadAsset("Mercs2Globals", "sounddb")
  Pg.UnloadAsset("MusicMarkers", "musicmarkers")
  Pg.UnloadAsset("MusicTransitions", "musictransitions")
  Pg.UnloadAsset("VehicleEngines", "animationtable")
  Pg.UnloadAsset("Sounds", "animationtable")
  Pg.UnloadAsset("SoundsAppendix", "animationtable")
  Pg.UnloadAsset("SoundMatch", "animationtable")
  Pg.UnloadAsset("SoundKey", "materialkeytable")
  CloseStreamsDLC()
end

UnloadRequiredAssetsDLC = L0_1

function L0_1()
  local L0_2, L1_2
  
  function L0_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3
    Sound.OpenStreamFile((Sound.GetAudioDir() .. "\\" .. _GetLocalizedName(_StripPWSExtension(A0_3)) .. ".pws"), A0_3)
  end
  
  _OpenFile = L0_2
  _OpenFile("vo_stream.pws")
  _OpenFile("ambience.pws")
  _OpenFile("music.pws")
end

OpenStreamsVZ = L0_1

function L0_1()
  local L0_2, L1_2
  
  function L0_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3
    Sound.CloseStreamFile((Sound.GetAudioDir() .. "\\" .. _GetLocalizedName(_StripPWSExtension(A0_3)) .. ".pws"), A0_3)
  end
  
  _CloseFile = L0_2
  _CloseFile("vo_stream.pws")
  _CloseFile("ambience.pws")
end

CloseStreamsVZ = L0_1

function L0_1()
  local L0_2, L1_2
  OpenStreamsVZ()
  MrxSoundBanks.LoadWaveBank("ambience")
  MrxSoundBanks.LoadSoundBank("ambience")
  MrxSoundBanks.LoadWaveBank("amb_birds")
  MrxSoundBanks.LoadSoundBank("amb_birds")
  MrxSoundBanks.LoadWaveBank("amb_shared")
  MrxSoundBanks.LoadWaveBank("collision_shared")
  MrxSoundBanks.LoadSoundBank("collision_shared")
  MrxSoundBanks.LoadWaveBank("destruction_shared")
  MrxSoundBanks.LoadSoundBank("destruction_shared")
  MrxSoundBanks.LoadWaveBank("fol_shared")
  MrxSoundBanks.LoadSoundBank("fol_shared")
  MrxSoundBanks.LoadSoundBank("veh_shared")
  MrxSoundBanks.LoadWaveBank("veh_shared")
  MrxSoundBanks.LoadSoundBank("wpn_shared")
  MrxSoundBanks.LoadWaveBank("wpn_shared")
  MrxSoundBanks.LoadSoundBank("building_destruct")
  MrxSoundBanks.LoadWaveBank("bulding_destruct")
  MrxSoundBanks.LoadSoundBank("veh_support")
  MrxSoundBanks.LoadWaveBank("veh_support")
  MrxSoundBanks.LoadWaveBank("vo_stream")
  MrxSoundBanks.LoadSoundBank("vo_mattias")
  MrxSoundBanks.LoadSoundBank("vo_Chris")
  MrxSoundBanks.LoadSoundBank("vo_carmona")
  MrxSoundBanks.LoadSoundBank("vo_Jen")
  MrxSoundBanks.LoadSoundBank("vo_Fiona")
  MrxSoundBanks.LoadSoundBank("vo_Ewan")
  MrxSoundBanks.LoadSoundBank("vo_Misha")
  MrxSoundBanks.LoadSoundBank("vo_Misc")
  MrxSoundBanks.LoadSoundBank("vo_alliedSoldier_01")
  MrxSoundBanks.LoadSoundBank("vo_alliedSoldier_02")
  MrxSoundBanks.LoadSoundBank("vo_alliedSoldier_black_03")
  MrxSoundBanks.LoadSoundBank("vo_chinSoldier_01")
  MrxSoundBanks.LoadSoundBank("vo_chinSoldier_02")
  MrxSoundBanks.LoadSoundBank("vo_oc_merc_01")
  MrxSoundBanks.LoadSoundBank("vo_oc_merc_02")
  MrxSoundBanks.LoadSoundBank("vo_vzCiv_01")
  MrxSoundBanks.LoadSoundBank("vo_vzCiv_02")
  MrxSoundBanks.LoadSoundBank("vo_vzCiv_female_01")
  MrxSoundBanks.LoadSoundBank("vo_vzCiv_female_02")
  MrxSoundBanks.LoadSoundBank("vo_vzGurSoldier_01")
  MrxSoundBanks.LoadSoundBank("vo_vzGurSoldier_02")
  MrxSoundBanks.LoadSoundBank("vo_vzGurSoldier_female_01")
  MrxSoundBanks.LoadSoundBank("vo_vzSoldier_01")
  MrxSoundBanks.LoadSoundBank("vo_vzSoldier_02")
  MrxSoundBanks.LoadSoundBank("vo_pirate_01")
  MrxSoundBanks.LoadSoundBank("vo_pirate_02")
  MrxSoundBanks.LoadSoundBank("vo_pirate_female_01")
end

LoadResidentAssetsVZ = L0_1

function L0_1()
  local L0_2, L1_2
  MrxSoundBanks.UnloadWaveBank("ambience")
  MrxSoundBanks.UnloadSoundBank("ambience")
  MrxSoundBanks.UnloadWaveBank("amb_birds")
  MrxSoundBanks.UnloadSoundBank("amb_birds")
  MrxSoundBanks.UnloadWaveBank("amb_shared")
  MrxSoundBanks.UnloadWaveBank("collision_shared")
  MrxSoundBanks.UnloadSoundBank("collision_shared")
  MrxSoundBanks.UnloadWaveBank("destruction_shared")
  MrxSoundBanks.UnloadSoundBank("destruction_shared")
  MrxSoundBanks.UnloadWaveBank("fol_shared")
  MrxSoundBanks.UnloadSoundBank("fol_shared")
  MrxSoundBanks.UnloadSoundBank("veh_shared")
  MrxSoundBanks.UnloadWaveBank("veh_shared")
  MrxSoundBanks.UnloadSoundBank("wpn_shared")
  MrxSoundBanks.UnloadWaveBank("wpn_shared")
  MrxSoundBanks.UnloadSoundBank("building_destruct")
  MrxSoundBanks.UnloadWaveBank("bulding_destruct")
  MrxSoundBanks.UnloadSoundBank("veh_support")
  MrxSoundBanks.UnloadWaveBank("veh_support")
  MrxSoundBanks.UnloadWaveBank("vo_stream")
  MrxSoundBanks.UnloadSoundBank("vo_mattias")
  MrxSoundBanks.UnloadSoundBank("vo_Chris")
  MrxSoundBanks.UnloadSoundBank("vo_carmona")
  MrxSoundBanks.UnloadSoundBank("vo_Jen")
  MrxSoundBanks.UnloadSoundBank("vo_Fiona")
  MrxSoundBanks.UnloadSoundBank("vo_Ewan")
  MrxSoundBanks.UnloadSoundBank("vo_Misha")
  MrxSoundBanks.UnloadSoundBank("vo_Misc")
  MrxSoundBanks.UnloadSoundBank("vo_alliedSoldier_01")
  MrxSoundBanks.UnloadSoundBank("vo_alliedSoldier_02")
  MrxSoundBanks.UnloadSoundBank("vo_alliedSoldier_black_03")
  MrxSoundBanks.UnloadSoundBank("vo_chinSoldier_01")
  MrxSoundBanks.UnloadSoundBank("vo_chinSoldier_02")
  MrxSoundBanks.UnloadSoundBank("vo_oc_merc_01")
  MrxSoundBanks.UnloadSoundBank("vo_oc_merc_02")
  MrxSoundBanks.UnloadSoundBank("vo_vzCiv_01")
  MrxSoundBanks.UnloadSoundBank("vo_vzCiv_02")
  MrxSoundBanks.UnloadSoundBank("vo_vzCiv_female_01")
  MrxSoundBanks.UnloadSoundBank("vo_vzCiv_female_02")
  MrxSoundBanks.UnloadSoundBank("vo_vzGurSoldier_01")
  MrxSoundBanks.UnloadSoundBank("vo_vzGurSoldier_02")
  MrxSoundBanks.UnloadSoundBank("vo_vzGurSoldier_female_01")
  MrxSoundBanks.UnloadSoundBank("vo_vzSoldier_01")
  MrxSoundBanks.UnloadSoundBank("vo_vzSoldier_02")
  MrxSoundBanks.UnloadSoundBank("vo_pirate_01")
  MrxSoundBanks.UnloadSoundBank("vo_pirate_02")
  MrxSoundBanks.UnloadSoundBank("vo_pirate_female_01")
  CloseStreamsVZ()
end

UnloadResidentAssetsVZ = L0_1
